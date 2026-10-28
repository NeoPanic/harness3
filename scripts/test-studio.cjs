const {test}=require('node:test');
const assert=require('node:assert/strict');
const {readFileSync}=require('node:fs');
const {resolve}=require('node:path');
const vm=require('node:vm');

const context=vm.createContext({document:{addEventListener(){}},events:[]});
Object.assign(context,{
  esc:(value='')=>String(value??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])),
  labels:{},icon:()=>'',time:()=>'',short:value=>String(value??'').slice(0,15),
});
vm.runInContext(readFileSync(resolve(__dirname,'../crates/harness3-platform/web/studio.js'),'utf8'),context);
function layout(tasks) {
  context.input=tasks;
  return vm.runInContext('studioTaskLayout(input)',context);
}

test('依赖层级不受 API 返回顺序影响',()=>{
  const {positions}=layout([
    {key:'summary',depends_on:['review']},
    {key:'review',depends_on:['source']},
    {key:'source',depends_on:[]},
  ]);
  assert(positions.get('source').x<positions.get('review').x);
  assert(positions.get('review').x<positions.get('summary').x);
});

test('并行分工共享层级，并在汇总前合流',()=>{
  const {positions}=layout([
    {key:'source',depends_on:[]},
    {key:'analysis-a',depends_on:['source']},
    {key:'analysis-b',depends_on:['source']},
    {key:'summary',depends_on:['analysis-a','analysis-b']},
  ]);
  const a=positions.get('analysis-a'),b=positions.get('analysis-b');
  assert.equal(a.x,b.x);
  assert.notEqual(a.y,b.y);
  assert(positions.get('summary').x>a.x);
});

test('派生子任务完成后，原任务续作再进入汇总',()=>{
  const {positions}=layout([
    {key:'parent',depends_on:['child-a','child-b'],delegation_scope:{depth:0}},
    {key:'summary',depends_on:['parent']},
    {key:'child-a',depends_on:[],delegation_scope:{parent_task:'parent'}},
    {key:'child-b',depends_on:[],delegation_scope:{parent_task:'parent'}},
  ]);
  assert(positions.get('child-a').x<positions.get('parent').x);
  assert(positions.get('parent').x<positions.get('summary').x);
});

test('64 个任务的画布完整覆盖全部工位',()=>{
  const {positions,width,height}=layout(Array.from({length:64},(_,i)=>({key:String(i),depends_on:i?[String(i-1)]:[]})));
  assert.equal(positions.size,64);
  for(const p of positions.values()){
    assert(p.x+240<=width);
    assert(p.y+145<=height);
  }
});

test('空图和不完整引用保留可读画布',()=>{
  assert.equal(layout([]).positions.size,0);
  const {positions}=layout([{key:'task',depends_on:['not-in-response']}]);
  assert.equal(positions.size,1);
  assert(Number.isFinite(positions.get('task').x));
});

function sceneFixture(){
  return {instances:[
    {id:'agent-a',preset_name:'队员 A',state:'ready',snapshot:{protocol:'demo'}},
    {id:'agent-b',preset_name:'队员 B',state:'busy',snapshot:{protocol:'demo'}},
  ],runs:[{id:'run-b',instance_id:'agent-b',state:'running',input:'检查数据'}],messages:[],goals:[],limits:{workers:8}};
}

test('默认场景没有说明段落或展开的详情，只让实际忙碌的队员打字',()=>{
  context.data=sceneFixture();
  const html=vm.runInContext('studioPage()',context);
  assert(!html.includes('<p>'));
  assert(!html.includes('<aside'));
  assert.equal((html.match(/is-working/g)||[]).length,1);
  assert(html.includes('1 工作'));
});

test('消息光点只出现在排队或消费中的记录',()=>{
  context.input=sceneFixture();
  context.input.messages=[{sender_instance_id:'agent-a',recipient_id:'agent-b',inbox_state:'consumed'}];
  let html=vm.runInContext('studioNetwork(input)',context);
  assert(!html.includes('message-packet'));
  context.input.messages[0].inbox_state='running';
  html=vm.runInContext('studioNetwork(input)',context);
  assert.equal((html.match(/class="message-packet/g)||[]).length,3);
});

test('排队占用实例时，显示等待姿态，不提前打字',()=>{
  context.input=sceneFixture();context.input.runs[0].state='queued';
  const html=vm.runInContext('studioNetwork(input)',context);
  assert(!html.includes('is-working'));
  assert(html.includes('creature-wait'));
});

test('暂停时保留原始队员状态，不显示后来收到的任务',()=>{
  context.input=sceneFixture();
  vm.runInContext('studio.paused={data:input,events:[],goal:null,goalError:""}',context);
  context.data=sceneFixture();context.data.instances[0].state='busy';
  const html=vm.runInContext('studioPage()',context);
  assert(html.includes('1 工作'));
  assert(!html.includes('2 工作'));
  vm.runInContext('studio.paused=null',context);
});

test('角色名称和提示中的用户文本按 HTML 转义',()=>{
  context.input=sceneFixture();context.input.instances[0].preset_name='<img src=x onerror=alert(1)>';
  const html=vm.runInContext('studioNetwork(input)',context);
  assert(!html.includes('<img'));
  assert(html.includes('&lt;img'));
});
