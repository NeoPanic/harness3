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
vm.runInContext(readFileSync(resolve(__dirname,'../web/src/features/studio/studio.js'),'utf8'),context);
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

test('实例注册不会虚构连向中央设施的主从连线',()=>{
  context.input=sceneFixture();
  const html=vm.runInContext('studioNetwork(input)',context);
  assert(!html.includes('station-conduit'));
  assert(!html.includes('调度核心'));
  assert(html.includes('共享任务板'));
});

test('只有实际认领中的任务连接任务板与执行实例',()=>{
  context.input=sceneFixture();
  context.input.board={tasks:[
    {id:'t1',root_id:'t1',title:'等待选型',state:'pending'},
    {id:'t2',root_id:'t2',title:'检查数据',state:'running',instance_id:'agent-b'},
    {id:'t3',root_id:'t3',title:'过去任务',state:'completed',instance_id:'agent-a'},
  ]};
  const html=vm.runInContext('studioNetwork(input)',context);
  assert.equal((html.match(/class="station-conduit /g)||[]).length,1);
  assert(html.includes('已认领任务：检查数据'));
});

test('任务拆分图来自任务板父子记录，无固定规划或汇总身份',()=>{
  context.input=sceneFixture();
  context.input.board={tasks:[
    {id:'root',root_id:'root',title:'任务目标',state:'waiting'},
    {id:'child',root_id:'root',parent_id:'root',title:'细分任务',state:'pending'},
  ]};
  const html=vm.runInContext('studioTasks(input)',context);
  assert(html.includes('拆分来源'));
  assert(html.includes('细分任务'));
  assert(!html.includes('执行依赖'));
  assert(!html.includes('#research'));
});
