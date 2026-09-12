struct Body { p: vec4<f32>, v: vec4<f32> }
struct Params { a: vec4<f32>, b: vec4<f32>, mouse: vec4<f32> }
@group(0) @binding(0) var<storage, read> input: array<Body>;
@group(0) @binding(1) var<storage, read_write> output: array<Body>;
@group(0) @binding(2) var<uniform> params: Params;
fn axis(t:f32) -> vec2<f32> { return vec2<f32>(cos(t),sin(t)); }
fn radius(t:f32,n:vec2<f32>) -> f32 {
 let u=axis(t); let v=vec2<f32>(-u.y,u.x);
 return 0.5*(abs(dot(u,n))+abs(dot(v,n)));
}
@compute @workgroup_size(64)
fn step(@builtin(global_invocation_id) id:vec3<u32>) {
 let i=id.x; let count=u32(params.a.x); if(i>=count){return;}
 let dt=params.a.y; let side=params.a.z;
 let me=input[i]; var force=vec2<f32>(0.0,-params.a.w*9.0); var torque=0.0;
 for(var j=0u;j<count;j++) {
  if(i==j){continue;} let other=input[j]; let delta=me.p.xy-other.p.xy;
  let d2=dot(delta,delta)+0.15;
  force-=params.b.x*delta/(d2*sqrt(d2));
  let u=axis(me.p.z); let v=axis(other.p.z);
  let axes=array<vec2<f32>,4>(u,vec2<f32>(-u.y,u.x),v,vec2<f32>(-v.y,v.x));
  var depth=10000.0; var normal=vec2<f32>(1.0,0.0);
  for(var k=0u;k<4u;k++) {
   let n=axes[k]; let overlap=radius(me.p.z,n)+radius(other.p.z,n)-abs(dot(delta,n));
   if(overlap<depth){depth=overlap; normal=n*select(-1.0,1.0,dot(delta,n)>=0.0);}
  }
  if(depth>0.0){
   let speed=dot(me.v.xy-other.v.xy,normal);
   let strength=max(0.0,params.b.z*depth-12.0*speed);
   force+=normal*strength;
   // Contact lever arm on the moving square gives playful angular response.
   let tangent=vec2<f32>(-normal.y,normal.x);
   let lever=clamp(dot(-delta*0.5,tangent),-0.5,0.5);
   torque-=lever*strength*2.0;
  }
 }
 let h=radius(me.p.z,vec2<f32>(1.0,0.0));
 if(me.p.x<h){force.x+=params.b.z*(h-me.p.x)-12.0*me.v.x;}
 if(me.p.y<h){force.y+=params.b.z*(h-me.p.y)-12.0*me.v.y;}
 if(me.p.x>side-h){force.x-=params.b.z*(me.p.x-side+h)+12.0*me.v.x;}
 if(me.p.y>side-h){force.y-=params.b.z*(me.p.y-side+h)+12.0*me.v.y;}
 if(i==u32(params.mouse.z) && params.mouse.w>0.0){
  let delta=params.mouse.xy-me.p.xy;
  let gain=100.0*min(1.0,0.4/max(0.0001,length(delta)));
  force+=delta*gain-me.v.xy*14.0;
 }
 var vel=(me.v.xy+force*dt)*exp(-params.b.y*dt);
 vel=clamp(vel,vec2<f32>(-15.0),vec2<f32>(15.0));
 let omega=clamp((me.v.z+torque*dt)*exp(-(params.b.y+3.0)*dt),-8.0,8.0);
 output[i].p=vec4<f32>(me.p.xy+vel*dt,me.p.z+omega*dt,0.0);
 output[i].v=vec4<f32>(vel,omega,0.0);
}
