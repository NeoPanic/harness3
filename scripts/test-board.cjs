const {test}=require('node:test');
const assert=require('node:assert/strict');
const {readFileSync}=require('node:fs');
const {resolve}=require('node:path');
const vm=require('node:vm');
const context=vm.createContext({location:{hash:'#board'}});
Object.assign(context,{
  esc:(value='')=>String(value??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])),
  badge:(state,text)=>`<span>${text}</span>`,button:(label,action)=>`<button data-action="${action}">${label}</button>`,
  head:(_,title,description,actions)=>`<h1>${title}</h1><p>${description}</p>${actions}`,
  panel:(title,body,extra='')=>`<section><h2>${title}</h2>${extra}${body}</section>`,
  empty:(title,description,action='')=>`${title}${description}${action}`,
  formEnd:label=>`<button type="submit">${label}</button>`,
  openModal:(title,html)=>{context.form=html;},dataView:value=>JSON.stringify(value),
});
vm.runInContext(readFileSync(resolve(__dirname,'../crates/harness3-platform/web/board.js'),'utf8'),context);

test('发布表单只收集任务，不要求选择 preset 或实例',()=>{
  vm.runInContext('boardPublishForm()',context);
  assert(context.form.includes('name="instructions"'));
  assert(context.form.includes('name="acceptance"'));
  assert(!context.form.includes('<select'));
  assert(!context.form.includes('name="preset"'));
  assert(!context.form.includes('name="instance_id"'));
});

test('空任务板提供发布入口，快照尚无 board 时可正常显示',()=>{
  context.data={};
  const html=vm.runInContext('boardPage()',context);
  assert(html.includes('发布第一个任务'));
  assert(html.includes('0 项待完成'));
});

test('已选型未认领和执行中状态分开显示',()=>{
  context.task={state:'pending'};
  assert.equal(vm.runInContext('boardStatus(task)',context),'等待选型');
  context.task.preset_name='analyst';
  assert.equal(vm.runInContext('boardStatus(task)',context),'等待认领');
  context.task.state='running';
  assert.equal(vm.runInContext('boardStatus(task)',context),'执行中');
});

test('任务详情显示拆分来源与子任务，同时转义任务内容',()=>{
  context.location.hash='#board/root';
  context.data={board:{tasks:[
    {id:'root',root_id:'root',title:'<script>bad</script>',instructions:'<img src=x>',acceptance:'完成',state:'waiting'},
    {id:'child',root_id:'root',parent_id:'root',title:'子任务',state:'pending'},
  ]}};
  const html=vm.runInContext('boardPage()',context);
  assert(!html.includes('<script>'));
  assert(!html.includes('<img'));
  assert(html.includes('子任务'));
  assert(html.includes('board-cancel'));
  context.location.hash='#board/child';
  assert(vm.runInContext('boardPage()',context).includes('拆分来源'));
});

test('父任务重新选型时不把历史 preset 显示成已分配',()=>{
  context.task={state:'pending',preset_name:'old-preset',route_state:'unassigned'};
  assert.equal(vm.runInContext('boardStatus(task)',context),'等待选型');
  assert.equal(vm.runInContext('boardPreset(task)',context),null);
});

test('选型失败显示重试原因，不会伪装为执行中',()=>{
  context.location.hash='#board';
  context.data={board:{tasks:[{id:'retry',title:'待重试任务',state:'pending',route_state:'unassigned'}],routing:[{task_id:'retry',state:'waiting',error:'router_unavailable',attempts:2}]}};
  const html=vm.runInContext('boardPage()',context);
  assert(html.includes('选型待重试'));
  assert(html.includes('router_unavailable'));
});
