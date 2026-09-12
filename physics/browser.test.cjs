// Optional integration test: run against a local packit server.
// npm install --prefix /tmp/packit-browser playwright
// NODE_PATH=/tmp/packit-browser/node_modules PACKIT_URL=http://localhost:8093 node physics/browser.test.cjs
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const base=process.env.PACKIT_URL||'http://localhost:8093';
(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--enable-unsafe-webgpu','--use-angle=swiftshader']});
 try {
  const page=await browser.newPage({viewport:{width:1280,height:1000}}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base+'/play/5');await page.waitForSelector('.pg-side');
  await page.waitForFunction(()=>document.querySelector('.pg-mode').textContent.includes('WebGPU compute'));
  // Test the actual GPU kernel against the independently exercised CPU path.
  await page.route('**/__physics.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'engine.js'),'utf8')}));
  const shader=fs.readFileSync(path.join(__dirname,'kernel.wgsl'),'utf8');
  const comparison=await page.evaluate(async shader=>{
   const {PackingPhysics}=await import('/__physics.js');
   const gpu=new PackingPhysics(4,4),cpu=new PackingPhysics(4,4);
   gpu.gravity=cpu.gravity=true;gpu.attraction=cpu.attraction=true;
   gpu.data[0]=1.4;gpu.data[8]=2.2;gpu.data[2]=.15;gpu.data[10]=-.1;cpu.data.set(gpu.data);
   await gpu.init(shader);await gpu.step(1);cpu.cpuStep(1/120);
   const maxError=Math.max(...gpu.data.map((v,i)=>Math.abs(v-cpu.data[i])));
   const mode=gpu.mode;
   // Simulate an edit during GPU readback. It must not be overwritten.
   const pending=gpu.step(2);gpu.revision++;gpu.data[0]=3.0;await pending;const editPreserved=gpu.data[0]===3.0;
   gpu.dispose();cpu.dispose();return {maxError,mode,editPreserved};
  },shader);
  assert.equal(comparison.mode,'WebGPU compute');/* GPU transcendental precision differs from JS double precision; velocities agree within 0.005 units/s. */ assert(comparison.maxError<.005,JSON.stringify(comparison));assert(comparison.editPreserved);console.log('GPU/CPU parity:',comparison);
  const side=2+1/Math.sqrt(2),sq=(cx,cy,theta=0)=>({cx,cy,theta});
  const arrangement={n:5,side,squares:[sq(.5,.5),sq(side-.5,.5),sq(.5,side-.5),sq(side-.5,side-.5),sq(side/2,side/2,Math.PI/4)]};
  await page.locator('.pg-file').setInputFiles({name:'five.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(arrangement))});
  await page.waitForFunction(()=>document.querySelector('.pg-status').textContent.includes('Imported'));
  await page.locator('.pg-refine').click();await page.waitForFunction(()=>document.querySelector('.pg-status').textContent.includes('Ready'));
  const player='browser-'+Date.now();await page.locator('.pg-player').fill(player);await page.locator('.pg-save').click();
  await page.waitForFunction(()=>document.querySelector('.pg-status').textContent.includes('Saved!'));
  const rows=await (await page.request.get(base+'/api/scores?n=5&limit=200')).json();const row=rows.find(r=>r.player===player);assert(row,'submitted score appears');assert(Math.abs(row.side-side)<1e-6);
  const detail=await (await page.request.get(base+'/api/scores/'+row.id)).json();assert.equal(detail.arrangement.squares.length,5);console.log('Database submission:',row.side,row.rank);
  await page.goto(base+'/score/'+row.id);await page.waitForSelector('svg.arrangement');assert.equal(await page.locator('svg polygon').count(),5);
  await page.goto(base+'/play/5');await page.waitForSelector('canvas');await page.setViewportSize({width:390,height:844});
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'no mobile overflow');
  if(process.env.PACKIT_SCREENSHOT)await page.screenshot({path:process.env.PACKIT_SCREENSHOT,fullPage:true});
  assert.deepEqual(errors,[]);
  const fallback=await browser.newPage();await fallback.addInitScript(()=>Object.defineProperty(navigator,'gpu',{get:()=>undefined}));await fallback.goto(base+'/play/4');await fallback.waitForSelector('.pg-side');
  assert((await fallback.locator('.pg-mode').textContent()).includes('CPU fallback'));
  await fallback.locator('.pg-gravity').check();await fallback.locator('.pg-refine').click();await fallback.waitForFunction(()=>document.querySelector('.pg-status').textContent.includes('Ready'));
  console.log('Browser integration passed (GPU, fallback, import, refine, submit, detail, mobile).');
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
