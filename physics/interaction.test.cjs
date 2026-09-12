// Read-only browser regression: PACKIT_URL=http://localhost:8093 node physics/interaction.test.cjs
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--enable-unsafe-webgpu','--use-angle=swiftshader']});
 try{
  for(const gpu of [true,false]){
   const page=await browser.newPage({viewport:{width:1280,height:1000}}),errors=[];
   page.on('pageerror',e=>errors.push(e.message));
   await page.route('**/__interaction',r=>r.fulfill({contentType:'text/html',body:'<div id="root" class="packing-game"></div>'}));
   for(const [url,file] of [['engine','engine.js'],['game','../frontend/src/game/game.js']])await page.route(`**/__${url}.js`,r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,file),'utf8')}));
   await page.route('**/api/records',r=>r.fulfill({json:[]}));
   if(!gpu)await page.addInitScript(()=>Object.defineProperty(navigator,'gpu',{get:()=>undefined}));
   await page.goto((process.env.PACKIT_URL||'http://localhost:8093')+'/__interaction');
   await page.evaluate(async shader=>{
    const {PackingPhysics}=await import('/__engine.js'),{mountGame}=await import('/__game.js');
    const p=window.testPhysics=new PackingPhysics(2,4);
    p.data[0]=1;p.data[1]=2;p.data[8]=2.1;p.data[9]=2;p.paused=true;
    window.testGame=mountGame(document.querySelector('#root'),p,shader,()=>JSON.stringify({error:'No automatic measurement during test'}),()=>{});
   },fs.readFileSync(path.join(__dirname,'kernel.wgsl'),'utf8'));
   if(gpu)await page.waitForFunction(()=>window.testPhysics.mode==='WebGPU compute');
   await page.waitForFunction(()=>document.querySelector('canvas').width===document.querySelector('canvas').height);
   const box=await page.locator('canvas').boundingBox();
   const point=(x,y)=>({x:box.x+box.width*(.045+.91*x/4),y:box.y+box.height*(.955-.91*y/4)});
   const start=point(1,2),end=point(3.9,2);
   await page.mouse.move(start.x,start.y);await page.mouse.down();
   assert.equal(await page.evaluate(()=>window.testPhysics.paused),false,'grabbing wakes a paused scene');
   await page.mouse.move(end.x,end.y,{steps:60});
   await page.waitForFunction(()=>window.testPhysics.data[8]>3.25);
   const dragged=await page.evaluate(()=>({x:window.testPhysics.data[0],neighbor:window.testPhysics.data[8],revision:window.testPhysics.revision}));
   assert(dragged.neighbor-dragged.x>.85,JSON.stringify(dragged));assert(dragged.x<2.8);assert.equal(dragged.revision,0);
   await page.mouse.up();
   await page.locator('.pg-band').fill('40');await page.locator('.pg-band').dispatchEvent('input');
   await page.locator('.pg-size').fill('3');await page.locator('.pg-size').dispatchEvent('input');
   await page.waitForFunction(()=>window.testPhysics.side<3.8);
   assert.equal(await page.evaluate(()=>window.testPhysics.targetSide),3);
   await page.locator('.pg-band').fill('0');await page.locator('.pg-band').dispatchEvent('input');
   const side=await page.evaluate(()=>window.testPhysics.side);await page.waitForTimeout(100);
   assert.equal(await page.evaluate(()=>window.testPhysics.side),side);
   assert.deepEqual(errors,[]);console.log(gpu?'GPU interaction passed':'CPU interaction passed',dragged);
   await page.evaluate(()=>window.testGame.dispose());await page.close();
  }
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
