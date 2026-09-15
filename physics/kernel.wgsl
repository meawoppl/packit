struct Body { p: vec4<f32>, v: vec4<f32> }
struct Params { a: vec4<f32>, b: vec4<f32>, mouse: vec4<f32>, rotation: vec4<f32>, glue: vec4<f32> }
@group(0) @binding(0) var<storage, read> input: array<Body>;
@group(0) @binding(1) var<storage, read_write> output: array<Body>;
@group(0) @binding(2) var<uniform> params: Params;
fn axis(t:f32) -> vec2<f32> { return vec2<f32>(cos(t),sin(t)); }
fn radius(t:f32,n:vec2<f32>) -> f32 {
 let u=axis(t); let v=vec2<f32>(-u.y,u.x);
 return 0.5*(abs(dot(u,n))+abs(dot(v,n)));
}
// Facing edge anchors share the same 0.75-unit range for bodies and walls.
fn normals(t:f32)->array<vec2<f32>,4>{let u=axis(t);return array<vec2<f32>,4>(u,vec2<f32>(-u.y,u.x),-u,vec2<f32>(u.y,-u.x));}
fn cross(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
fn edge_pull(n:vec2<f32>,opposite:vec2<f32>,delta:vec2<f32>,strength:f32)->vec3<f32>{
 let distance=length(delta);
 if(distance>=0.75 || dot(n,opposite)>-0.5 || dot(n,delta)<0.0 || dot(opposite,delta)>0.0){return vec3<f32>(0.0);}
 let falloff=1.0-distance/0.75;let weight=falloff*falloff;
 let force=strength*weight*delta;
 let torque=6.0*(0.5*cross(n,force)-strength*0.15*weight*cross(n,opposite));
 return vec3<f32>(force,torque);
}
fn pair_edges(me:Body,other:Body,strength:f32)->vec3<f32>{
 var result=vec3<f32>(0.0);let d=me.p.xy-other.p.xy;
 if(strength==0.0 || dot(d,d)>4.7){return result;}
 let a=normals(me.p.z);let b=normals(other.p.z);
 for(var i=0u;i<4u;i++){for(var j=0u;j<4u;j++){
  let delta=other.p.xy+0.5*b[j]-me.p.xy-0.5*a[i];
  result+=edge_pull(a[i],b[j],delta,strength);
 }}return result;
}
fn wall_edges(me:Body,side:f32,strength:f32)->vec3<f32>{
 var result=vec3<f32>(0.0);if(strength==0.0){return result;}
 let h=radius(me.p.z,vec2<f32>(1.0,0.0));let a=normals(me.p.z);
 let inward=array<vec2<f32>,4>(vec2<f32>(1.0,0.0),vec2<f32>(0.0,1.0),vec2<f32>(-1.0,0.0),vec2<f32>(0.0,-1.0));
 let gaps=array<f32,4>(me.p.x-h,me.p.y-h,side-me.p.x-h,side-me.p.y-h);
 for(var i=0u;i<4u;i++){
  let anchor=me.p.xy+0.5*a[i];let clipped=clamp(anchor,vec2<f32>(0.0),vec2<f32>(side));
  let points=array<vec2<f32>,4>(vec2<f32>(0.0,clipped.y),vec2<f32>(clipped.x,0.0),vec2<f32>(side,clipped.y),vec2<f32>(clipped.x,side));
  for(var j=0u;j<4u;j++){if(gaps[j]>=0.0){result+=edge_pull(a[i],inward[j],points[j]-anchor,strength);}}
 }return result;
}
fn feature(me:Body,d:vec2<f32>,t:vec2<f32>,skin:f32)->vec2<f32>{
 let u=axis(me.p.z);let v=vec2<f32>(-u.y,u.x);let a=dot(u,d);let b=dot(v,d);
 let r=0.5*(select(sign(a),0.0,abs(a)<=skin)*u+select(sign(b),0.0,abs(b)<=skin)*v);
 let center=dot(me.p.xy+r,t);
 let extent=0.5*(select(0.0,abs(dot(u,t)),abs(a)<=skin)+select(0.0,abs(dot(v,t)),abs(b)<=skin));
 return vec2<f32>(center-extent,center+extent);
}
fn contact_levers(me:Body,other:Body,n:vec2<f32>,depth:f32)->vec3<f32>{
 let t=vec2<f32>(-n.y,n.x);let skin=max(0.00001,depth);let a=feature(me,-n,t,skin);let b=feature(other,n,t,skin);
 let contact=0.5*(max(a.x,b.x)+min(a.y,b.y));
 return vec3<f32>(contact-dot(me.p.xy,t),contact-dot(other.p.xy,t),max(0.0,min(a.y,b.y)-max(a.x,b.x)));
}
fn wall_contact(me:Body,n:vec2<f32>,depth:f32)->vec3<f32>{
 var result=vec3<f32>(0.0);if(depth<=0.0){return result;}
 let u=axis(me.p.z);let v=vec2<f32>(-u.y,u.x);let h=0.5*(abs(dot(u,n))+abs(dot(v,n)));
 var total=0.0;
 for(var i=0u;i<2u;i++){for(var j=0u;j<2u;j++){
  let r=0.5*((f32(i)*2.0-1.0)*u+(f32(j)*2.0-1.0)*v);
  total+=max(0.0,depth-dot(r,n)-h);
 }}
 let weight=depth/max(total,1e-12);
 for(var i=0u;i<2u;i++){for(var j=0u;j<2u;j++){
  let r=0.5*(select(-1.0,1.0,i==1u)*u+select(-1.0,1.0,j==1u)*v);
  let penetration=depth-dot(r,n)-h;if(penetration<=0.0){continue;}
  let spin=cross(r,n);let speed=dot(me.v.xy,n)+me.v.z*spin;
  let strength=weight*max(0.0,params.b.z*penetration-12.0*speed);
  result+=vec3<f32>(n*strength,spin*strength*6.0);
 }}return result;
}
struct GlueInput { a:vec4<u32>, b:vec4<u32> }
@group(0) @binding(3) var<storage,read> glues:array<GlueInput>;
struct GlueWorld { p:vec2<f32>, n:vec2<f32>, half:f32, owner:u32 }
fn tangent(n:vec2<f32>)->vec2<f32>{return vec2<f32>(-n.y,n.x);}
fn glue_world(f:vec4<u32>)->GlueWorld {
 let side=params.a.z;
 if(f.x==3u){
  let positions=array<vec2<f32>,4>(vec2<f32>(0.0,side/2.0),vec2<f32>(side/2.0,0.0),vec2<f32>(side,side/2.0),vec2<f32>(side/2.0,side));
  let ns=array<vec2<f32>,4>(vec2<f32>(1.0,0.0),vec2<f32>(0.0,1.0),vec2<f32>(-1.0,0.0),vec2<f32>(0.0,-1.0));
  return GlueWorld(positions[f.z],ns[f.z],side/2.0,0xffffffffu);
 }
 let body=input[f.y];let ns=normals(body.p.z);
 if(f.x==0u){return GlueWorld(body.p.xy+0.5*ns[f.z],ns[f.z],0.5,f.y);}
 if(f.x==2u){return GlueWorld(body.p.xy+0.5*ns[f.z],vec2<f32>(0.0),0.0,f.y);}
 let offset=select(-0.5,0.5,(f.z&1u)!=0u)*ns[0]+select(-0.5,0.5,(f.z&2u)!=0u)*ns[1];
 return GlueWorld(body.p.xy+offset,vec2<f32>(0.0),0.0,f.y);
}
fn glue_velocity(w:GlueWorld,p:vec2<f32>)->vec2<f32>{
 if(w.owner!=0xffffffffu){let b=input[w.owner];return b.v.xy+b.v.z*tangent(p-b.p.xy);}
 return params.glue.y*select(vec2<f32>(0.0),vec2<f32>(1.0),p>=vec2<f32>(params.a.z-1e-6));
}
fn glue_omega(w:GlueWorld)->f32{if(w.owner!=0xffffffffu){return input[w.owner].v.z;}return 0.0;}
fn glue_point(w:GlueWorld,t:vec2<f32>,q:f32)->vec2<f32>{
 let d=dot(tangent(w.n),t);var offset=0.0;
 if(abs(d)>0.1){offset=clamp((q-dot(w.p,t))/d,-w.half,w.half);}
 return w.p+tangent(w.n)*offset;
}
fn glue_force(g:GlueInput,i:u32)->vec3<f32>{
 if(g.a.y!=i && g.b.y!=i){return vec3<f32>(0.0);}
 var a=glue_world(g.a);var b=glue_world(g.b);
 if(a.half>0.0 && b.half==0.0){let temp=a;a=b;b=temp;}
 var pa=a.p;var pb=b.p;var normal=vec2<f32>(0.0);var sliding=false;var couple=0.0;
 if(a.half>0.0 && b.half>0.0){
  normal=a.n-b.n;let len=length(normal);if(len>0.001){normal/=len;}else{normal=a.n;}
  let t=tangent(normal);let ha=a.half*abs(dot(tangent(a.n),t));let hb=b.half*abs(dot(tangent(b.n),t));
  let lo=max(dot(a.p,t)-ha,dot(b.p,t)-hb);let hi=min(dot(a.p,t)+ha,dot(b.p,t)+hb);
  let q=(lo+hi)*0.5;pa=glue_point(a,t,q);pb=glue_point(b,t,q);sliding=lo<=hi;
  let sine=cross(a.n,-b.n);let cosine=dot(a.n,-b.n);var angle=atan2(sine,cosine);
  if(abs(sine)<1e-6 && cosine<0.0){angle=3.141592653589793;}
  couple=clamp(params.b.z/24.0*angle+4.0*(glue_omega(b)-glue_omega(a)),-12.0,12.0);
 }else if(b.half>0.0){
  normal=b.n;let t=tangent(b.n);let q=dot(a.p-b.p,t);
  pb=b.p+t*clamp(q,-b.half,b.half);sliding=abs(q)<=b.half;
 }
 let speed=glue_velocity(b,pb)-glue_velocity(a,pa);
 var force=(pb-pa)*params.b.z+speed*18.0;
 if(sliding){force=normal*dot(force,normal);}
 let weight=1.0/f32(max(1u,max(g.a.w,g.b.w)));
 force*=min(1.0,80.0/max(length(force),0.0001))*weight;couple*=weight;
 if(a.owner==i){return vec3<f32>(force,6.0*(cross(pa-input[i].p.xy,force)+couple));}
 return vec3<f32>(-force,6.0*(cross(pb-input[i].p.xy,-force)-couple));
}

@compute @workgroup_size(64)
fn step(@builtin(global_invocation_id) id:vec3<u32>) {
 let i=id.x; let count=u32(params.a.x); if(i>=count){return;}
 let dt=params.a.y; let side=params.a.z;
 let me=input[i]; var force=vec2<f32>(0.0); var torque=0.0; var contact=vec2<f32>(0.0);
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
   let levers=contact_levers(me,other,normal,depth);
   let speed=dot(me.v.xy-other.v.xy,normal)-me.v.z*levers.x+other.v.z*levers.y;
   let strength=max(0.0,params.b.z*depth-12.0*speed);
   force+=normal*strength;contact+=normal*strength;torque-=levers.x*strength*6.0;
   torque-=levers.z*levers.z/12.0*6.0*(params.b.z*sin(4.0*(me.p.z-other.p.z))/4.0+12.0*(me.v.z-other.v.z));
  }else if(params.b.w>0.0){
   let pull=pair_edges(me,other,params.b.w);force+=pull.xy;contact+=pull.xy;torque+=pull.z;
  }
 }
 let h=radius(me.p.z,vec2<f32>(1.0,0.0));
 let walls=wall_contact(me,vec2<f32>(1.0,0.0),h-me.p.x)+wall_contact(me,vec2<f32>(0.0,1.0),h-me.p.y)+wall_contact(me,vec2<f32>(-1.0,0.0),me.p.x-side+h)+wall_contact(me,vec2<f32>(0.0,-1.0),me.p.y-side+h);
 let pull=wall_edges(me,side,params.b.w);force+=walls.xy+pull.xy;contact+=walls.xy+pull.xy;torque+=walls.z+pull.z;
 for(var g=0u;g<u32(params.glue.x);g++){let f=glue_force(glues[g],i);force+=f.xy;contact+=f.xy;torque+=f.z;}
 if(i==u32(params.mouse.z) && params.mouse.w>0.0){
  let delta=params.mouse.xy-me.p.xy;
  let gain=100.0*min(1.0,0.5/max(0.0001,length(delta)));
  force+=delta*gain-me.v.xy*14.0;
 }
 if(i==u32(params.rotation.y) && params.rotation.z>0.0){
  let error=params.rotation.x-me.p.z;
  torque+=clamp(60.0*atan2(sin(error),cos(error))-8.0*me.v.z,-30.0,30.0);
 }
 var vel=(me.v.xy+force*dt)*exp(-params.b.y*dt);
 vel=clamp(vel,vec2<f32>(-15.0),vec2<f32>(15.0));
 let omega=clamp((me.v.z+torque*dt)*exp(-(params.b.y+3.0)*dt),-8.0,8.0);
 output[i].p=vec4<f32>(me.p.xy+vel*dt,me.p.z+omega*dt,contact.x);
 output[i].v=vec4<f32>(vel,omega,contact.y);
}
