import test from 'node:test';
import assert from 'node:assert/strict';
import { PackingPhysics } from './engine.js';

test('gravity accelerates downward and damping stays finite',()=>{
 const p=new PackingPhysics(1,5);p.gravity=true;const y=p.data[1];for(let i=0;i<120;i++)p.cpuStep(1/120);
 assert(p.data[1]<y);assert([...p.data].every(Number.isFinite));assert(p.data[1]>.4);
});
test('separates overlapped rotated squares',()=>{
 const p=new PackingPhysics(2,5);p.data[0]=2;p.data[1]=2;p.data[2]=.15;p.data[8]=2.7;p.data[9]=2;p.data[10]=-.15;
 for(let i=0;i<240;i++)p.cpuStep(1/120);
 assert(Math.hypot(p.data[0]-p.data[8],p.data[1]-p.data[9])>.99);assert([...p.data].every(Number.isFinite));
});
test('attraction pulls disjoint squares together',()=>{
 const p=new PackingPhysics(2,8);p.data[0]=2;p.data[1]=4;p.data[8]=6;p.data[9]=4;p.attraction=true;
 for(let i=0;i<120;i++)p.cpuStep(1/120);
 assert(p.data[8]-p.data[0]<4);
});
test('mouse spring moves selected square toward pointer',()=>{
 const p=new PackingPhysics(1,5);p.mouse={x:3.5,y:3.5,index:0,down:true};for(let i=0;i<120;i++)p.cpuStep(1/120);
 assert(Math.abs(p.data[0]-3.5)<.05);assert(Math.abs(p.data[1]-3.5)<.05);
});
test('pause preserves state and reset clears momentum',async()=>{
 const p=new PackingPhysics(4,3);p.gravity=true;p.paused=true;const initial=p.data.slice();await p.step();assert.deepEqual(p.data,initial);
 p.paused=false;await p.step();assert.notDeepEqual(p.data,initial);p.reset();assert.deepEqual(p.data,initial);
});
test('high stiffness stays bounded under sustained gravity',()=>{
 const p=new PackingPhysics(16,4.5);p.gravity=true;p.attraction=true;p.stiffness=1600;p.damping=.3;
 for(let i=0;i<1200;i++)p.cpuStep(1/120);
 assert([...p.data].every(Number.isFinite));for(let i=0;i<p.n;i++){assert(p.data[i*8]>-.1&&p.data[i*8]<4.6);assert(p.data[i*8+1]>-.1&&p.data[i*8+1]<4.6);}
});
test('mouse pushes a neighbor and resists dragging through a wall',()=>{
 const p=new PackingPhysics(2,4);p.data[0]=1;p.data[1]=2;p.data[8]=2.1;p.data[9]=2;
 p.mouse={x:10,y:2,index:0,down:true};
 for(let i=0;i<600;i++)p.cpuStep(1/120);
 assert(p.data[8]>3.3,'neighbor is pushed toward the wall');
 assert(p.data[8]-p.data[0]>.9,'grabbed square does not pass through its neighbor');
 assert(p.data[0]<2.7,'blocked square resists the pointer');
 assert(p.data[8]<3.6,'wall contact stays stiff');
});
test('band contracts to its target and squares push it outward',()=>{
 const emptySpace=new PackingPhysics(1,5);emptySpace.bandTension=40;emptySpace.targetSide=3;
 emptySpace.data[0]=1;emptySpace.data[1]=1;
 for(let i=0;i<1200;i++)emptySpace.cpuStep(1/120);
 assert(Math.abs(emptySpace.side-3)<.01);
 const packed=new PackingPhysics(2,2);packed.bandTension=40;packed.targetSide=Math.sqrt(2);
 packed.data[0]=.5;packed.data[1]=.5;packed.data[8]=1.5;packed.data[9]=.5;
 for(let i=0;i<1200;i++)packed.cpuStep(1/120);
 assert(packed.side>1.8,'contacts support the band above its target');
 assert(packed.side<2.01);assert([...packed.data].every(Number.isFinite));
 packed.bandTension=0;const side=packed.side;
 for(let i=0;i<120;i++)packed.cpuStep(1/120);
 assert.equal(packed.side,side,'zero tension holds the boundary fixed');
});
