// Unit squares, bottom-left origin, radians. The GPU computes all pair forces.
export class PackingPhysics {
  constructor(n, side) {
    this.n=n; this.side=side; this.data=new Float32Array(n*8);
    this.gravity=false; this.attraction=false; this.damping=3; this.stiffness=900;
    this.mouse={x:0,y:0,index:-1,down:false}; this.mode='CPU fallback';
    this.paused=false; this.disposed=false; this.revision=0; this.reset();
  }
  reset() {
    const cols=Math.ceil(Math.sqrt(this.n));
    for(let i=0;i<this.n;i++) {const k=i*8;this.data[k]=(i%cols+.5)*this.side/cols;this.data[k+1]=(Math.floor(i/cols)+.5)*this.side/cols;this.data[k+2]=0;this.data.fill(0,k+3,k+8);}
  }
  async init(shader) {
    try {
      if(!navigator.gpu) return;
      const adapter=await navigator.gpu.requestAdapter(); if(!adapter)return;
      const device=await adapter.requestDevice(); if(this.disposed){device.destroy();return;}
      const module=device.createShaderModule({code:shader});
      this.pipeline=await device.createComputePipelineAsync({layout:'auto',compute:{module,entryPoint:'step'}});
      const size=this.data.byteLength;
      this.buffers=[0,1].map(()=>device.createBuffer({size,usage:GPUBufferUsage.STORAGE|GPUBufferUsage.COPY_DST|GPUBufferUsage.COPY_SRC}));
      this.uniform=device.createBuffer({size:48,usage:GPUBufferUsage.UNIFORM|GPUBufferUsage.COPY_DST});
      this.readback=device.createBuffer({size,usage:GPUBufferUsage.MAP_READ|GPUBufferUsage.COPY_DST});
      this.groups=this.buffers.map((b,i)=>device.createBindGroup({layout:this.pipeline.getBindGroupLayout(0),entries:[{binding:0,resource:{buffer:b}},{binding:1,resource:{buffer:this.buffers[1-i]}},{binding:2,resource:{buffer:this.uniform}}]}));
      if(this.disposed){device.destroy();return;}
      this.device=device;
      this.mode='WebGPU compute';
      device.lost.then(()=>{this.mode='CPU fallback';this.device=null;});
    } catch(e) {this.mode='CPU fallback';this.device?.destroy();this.device=null;console.info('WebGPU unavailable',e);}
  }
  async step(steps=2) {
    if(this.paused || this.disposed)return;
    if(!this.device){for(let s=0;s<steps;s++)this.cpuStep(1/120);return;}
    const device=this.device, revision=this.revision, initial=this.data.slice();
    try {
      device.queue.writeBuffer(this.buffers[0],0,this.data);
      device.queue.writeBuffer(this.uniform,0,new Float32Array([this.n,1/120,this.side,+this.gravity,+this.attraction*1.8,this.damping,this.stiffness,0,this.mouse.x,this.mouse.y,Math.max(0,this.mouse.index),+this.mouse.down]));
      const encoder=device.createCommandEncoder();
      for(let s=0;s<steps;s++){const pass=encoder.beginComputePass();pass.setPipeline(this.pipeline);pass.setBindGroup(0,this.groups[s%2]);pass.dispatchWorkgroups(Math.ceil(this.n/64));pass.end();}
      encoder.copyBufferToBuffer(this.buffers[steps%2],0,this.readback,0,this.data.byteLength);
      device.queue.submit([encoder.finish()]);
      await this.readback.mapAsync(GPUMapMode.READ);
      if(!this.disposed){
        const computed=new Float32Array(this.readback.getMappedRange());
        if(revision===this.revision)this.data.set(computed);
        else for(let i=0;i<this.data.length;i++)if(this.data[i]===initial[i])this.data[i]=computed[i];
      }
      this.readback.unmap();
    }catch(e){if(!this.disposed){this.mode='CPU fallback';this.device=null;console.info('GPU step failed',e);}}
  }
  cpuStep(dt) {
    const old=this.data, next=old.slice(), n=this.n, side=this.side;
    const radius=(t,x,y)=>.5*(Math.abs(Math.cos(t)*x+Math.sin(t)*y)+Math.abs(-Math.sin(t)*x+Math.cos(t)*y));
    for(let i=0;i<n;i++){
      const k=i*8,x=old[k],y=old[k+1],a=old[k+2],vx=old[k+4],vy=old[k+5];let fx=0,fy=this.gravity?-9:0,torque=0;
      for(let j=0;j<n;j++){
        if(i===j)continue;const l=j*8,dx=x-old[l],dy=y-old[l+1],b=old[l+2],d2=dx*dx+dy*dy+.15;
        if(this.attraction){fx-=1.8*dx/(d2*Math.sqrt(d2));fy-=1.8*dy/(d2*Math.sqrt(d2));}
        let depth=Infinity,nx=1,ny=0;
        for(const t of [a,a+Math.PI/2,b,b+Math.PI/2]){const ax=Math.cos(t),ay=Math.sin(t),dot=dx*ax+dy*ay;const overlap=radius(a,ax,ay)+radius(b,ax,ay)-Math.abs(dot);if(overlap<depth){depth=overlap;nx=ax*(dot>=0?1:-1);ny=ay*(dot>=0?1:-1);}}
        if(depth>0){const force=Math.max(0,this.stiffness*depth-12*((vx-old[l+4])*nx+(vy-old[l+5])*ny));fx+=nx*force;fy+=ny*force;const lever=Math.max(-.5,Math.min(.5,(dx*ny-dy*nx)*.5));torque-=lever*force*2;}
      }
      const h=radius(a,1,0);
      if(x<h)fx+=this.stiffness*(h-x)-12*vx;if(y<h)fy+=this.stiffness*(h-y)-12*vy;
      if(x>side-h)fx-=this.stiffness*(x-side+h)+12*vx;if(y>side-h)fy-=this.stiffness*(y-side+h)+12*vy;
      if(this.mouse.down&&this.mouse.index===i){fx+=(this.mouse.x-x)*100-vx*14;fy+=(this.mouse.y-y)*100-vy*14;}
      const clamp=v=>Math.max(-15,Math.min(15,v));next[k+4]=clamp((vx+fx*dt)*Math.exp(-this.damping*dt));next[k+5]=clamp((vy+fy*dt)*Math.exp(-this.damping*dt));next[k+6]=Math.max(-8,Math.min(8,(old[k+6]+torque*dt)*Math.exp(-(this.damping+3)*dt)));
      next[k]=x+next[k+4]*dt;next[k+1]=y+next[k+5]*dt;next[k+2]=a+next[k+6]*dt;
    }
    this.data=next;
  }
  arrangement(){return {n:this.n,side:this.side,squares:Array.from({length:this.n},(_,i)=>({cx:this.data[i*8],cy:this.data[i*8+1],theta:this.data[i*8+2]}))};}
  load(a){if(a.n!==this.n)throw Error('Square count mismatch');this.side=a.side;a.squares.forEach((s,i)=>{this.data[i*8]=s.cx;this.data[i*8+1]=s.cy;this.data[i*8+2]=s.theta;this.data.fill(0,i*8+3,i*8+8);});}
  dispose(){this.disposed=true;this.device?.destroy();}
}
export function createPhysics(n,side){return new PackingPhysics(n,side);}
