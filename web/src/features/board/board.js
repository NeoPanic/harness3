'use strict';

const boardStates={staged:'等待来源任务结算',pending:'等待选型',running:'执行中',waiting:'等待子任务',completed:'已完成',failed:'失败',cancelled:'已取消',blocked:'已阻断',interrupted:'结果未知'};
const boardTerminal=new Set(['completed','failed','cancelled','blocked','interrupted']);
let selectedBoard=null;
function boardId(){return location.hash.startsWith('#board/')?location.hash.slice(7):null;}
async function boardReadDetail(){
  const id=boardId();if(!id)return;
  try{const task=await api(`/api/board/tasks/${encodeURIComponent(id)}`);if(boardId()===id)selectedBoard=task;}
  catch(error){if(boardId()===id)selectedBoard={id,loadError:error.message};}
}
function boardData(snapshot=data){return snapshot.board||{tasks:[],agents:[]};}
function boardRoute(id){return (boardData().routing||[]).find(route=>route.task_id===id);}
function boardRoutingMessage(code){return ({board_no_suitable_preset:'暂无匹配的 Preset',decision_low_confidence:'选型置信度不足',board_worker_capacity:'等待执行容量',instance_quota_exceeded:'等待实例配额',decision_network_error:'选型服务连接失败',decision_invalid_response:'选型服务返回了无效结果'})[code]||code;}
function boardProvisioned(task){return task.route_state?task.route_state==='provisioned':Boolean(task.instance_id||task.preset_name);}
function boardPreset(task){return task.state==='pending'&&!boardProvisioned(task)?null:task.preset_name;}
function boardStatus(task,route){return task.state==='pending'?(boardProvisioned(task)?'等待认领':route?.state==='routing'?'正在选型':route?.error?'选型待重试':'等待选型'):boardStates[task.state]||task.state;}
function boardTaskRow(task){
  const route=boardRoute(task.id);
  return `<a class="board-task-row" href="#board/${esc(task.id)}"><span class="board-task-copy"><strong>${esc(task.title)}</strong>${task.instructions?`<span class="board-task-preview">${esc(task.instructions)}</span>`:''}<span>${esc(boardPreset(task)?`${task.preset_name} @ ${task.preset_version}`:route?.error?boardRoutingMessage(route.error):'控制面按任务选择 Preset')}${task.parent_id?' 子任务':''}</span></span>${badge(task.state,boardStatus(task,route))}<span aria-hidden="true">→</span></a>`;
}
function boardPage(){
  const board=boardData(),tasks=board.tasks||[],id=boardId();
  if(id){
    if(selectedBoard?.id===id&&selectedBoard.loadError)return `<a class="back-link" href="#board">← 任务板</a><div class="callout" role="alert">${esc(selectedBoard.loadError)} ${button('重试','board-refresh')}</div>`;
    const task=selectedBoard?.id===id?selectedBoard:tasks.find(t=>t.id===id);
    return task?boardDetail(task,tasks):`<a class="back-link" href="#board">← 任务板</a>${empty('任务未出现在当前快照','请返回任务板或刷新后重试。',button('刷新','board-refresh'))}`;
  }
  const roots=tasks.filter(t=>!t.parent_id),active=tasks.filter(t=>!boardTerminal.has(t.state));
  const history=roots.filter(t=>['completed','cancelled'].includes(t.state)),current=roots.filter(t=>!['completed','cancelled'].includes(t.state));
  return head('共享任务板', button('发布任务','board-publish',true))+
    (board.router?.error?`<div class="callout" role="status">选型暂不可用：${esc(board.router.error)}</div>`:'')+
    panel('当前任务',current.length?`<div class="board-task-list">${current.map(boardTaskRow).join('')}</div>`:empty(roots.length?'当前没有待处理任务':'还没有发布任务',roots.length?'历史任务保留在下方。发布新任务后，在这里跟进选型、执行与结果。':'点击右上角“发布任务”，填写目标与验收标准。'),`<span class="board-count">${active.length} 项待完成，含子任务</span>`)+
    (history.length?`<details id="board-history" class="board-history"><summary>历史任务 <span>${history.length} 项已完成或取消</span></summary><div class="board-task-list">${history.map(boardTaskRow).join('')}</div></details>`:'')+
    (board.history_truncated?`<p class="hint">显示最近 ${esc(board.roots_included)} 项根任务及其子任务，共 ${esc(board.roots_total)} 项根任务。活动任务优先。</p>`:'');
}
function boardDetail(task,tasks){
  const children=tasks.filter(t=>t.parent_id===task.id),parent=tasks.find(t=>t.id===task.parent_id);
  const route=boardRoute(task.id),routingError=task.state==='pending'&&route?.error;
  const result=typeof task.result==='string'?task.result:task.result?.output||task.result;
  return `<a class="back-link" href="#board">← 任务板</a>`+head(esc(task.title), !boardTerminal.has(task.state)?button('取消任务','board-cancel',false,`data-id="${esc(task.id)}"`):'')+
    `<div class="goal-summary"><div><span>进度</span>${badge(task.state,boardStatus(task,route))}</div><div><span>选中的 Preset</span><strong>${esc(boardPreset(task)?`${task.preset_name} @ ${task.preset_version}`:'待选型')}</strong></div><div><span>子任务</span><strong>${children.filter(t=>t.state==='completed').length} / ${children.length}</strong></div></div>`+
    (routingError?`<div class="callout" role="status" title="${esc(route.error)}">${esc(boardRoutingMessage(route.error))}${route.retry_at?`。下次尝试 ${esc(time(route.retry_at))}`:''}${route.attempts?`，已尝试 ${esc(route.attempts)} 次`:''}。</div>`:'')+
    (task.error?`<div class="callout" role="status">${esc(task.error)}</div>`:'')+
    panel('任务内容',`<div class="panel-body"><dl class="definition"><dt>目标</dt><dd>${esc(task.instructions)}</dd><dt>验收标准</dt><dd>${esc(task.acceptance)}</dd>${parent?`<dt>拆分来源</dt><dd><a href="#board/${esc(parent.id)}">${esc(parent.title)}</a></dd>`:''}</dl></div>`)+
    (children.length?panel('拆分后的任务',`<div class="board-task-list">${children.map(boardTaskRow).join('')}</div>`):'')+
    (task.route_reason?`<details class="raw-data"><summary>Preset 选择依据</summary><p class="record-text">${esc(task.route_reason)}</p></details>`:'')+
    (result?panel('执行结果',`<div class="panel-body output">${typeof result==='string'?esc(result):dataView(result)}</div>`):'')+
    `<div class="head-actions">${task.instance_id?button('认领实例','agent-detail',false,`data-id="${esc(task.instance_id)}"`):''}${task.run_id?button('执行记录','run-detail',false,`data-id="${esc(task.run_id)}"`):''}<a class="button" href="#studio">工作现场</a></div>`;
}
function boardPublishForm(){
  openModal('发布任务',`<form data-form="board-publish"><label>任务名称<input name="title" required maxlength="200" placeholder="例如：分析这个项目的性能瓶颈"></label><label>任务内容<textarea name="instructions" required maxlength="8000" rows="5" placeholder="描述目标、已有资料和约束。"></textarea></label><label>验收标准<textarea name="acceptance" required maxlength="8000" rows="3" placeholder="完成时应交付什么，如何判断结果可用？"></textarea></label>${formEnd('投递到任务板')}</form>`,'共享任务板');
}
async function boardAction(action,element){
  if(action==='board-publish'){boardPublishForm();return true;}
  if(action==='board-refresh'){await refresh(true);return true;}
  if(action==='board-cancel'){
    openModal('取消这项任务？',`<form data-form="board-cancel" data-id="${esc(element.dataset.id)}"><p>取消后停止后续认领，并请求正在执行的实例收尾。已产生的工具效果和历史记录保留。</p>${formEnd('确认取消')}</form>`,'任务生命周期');
    $('#modal').setAttribute('role','alertdialog');return true;
  }
  return false;
}
