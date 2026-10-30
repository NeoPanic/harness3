'use strict';
let selectedFleet=null;
function fleetId(){return location.hash.startsWith('#fleet/')?location.hash.slice(7):null;}
function fleetsPage(){
  const id=fleetId();
  if(id)return selectedFleet?.id===id?selectedFleet.loadError?`<a class="back-link" href="#fleets">← 全部 Agent 池</a><div class="callout" role="alert">${esc(selectedFleet.loadError)}</div>`:fleetPage(selectedFleet):'<div class="loading">正在读取 Agent 池与操作…</div>';
  const fleets=data.fleets||[],capacity=data.capacity;
  return head('Agent 池', button('创建 Agent 池','spawn',true))+
    `<div class="quota-summary"><span>已占用或预留 <strong>${capacity.reserved} <span>/ ${capacity.limit}</span></strong></span><span>${capacity.active} 个实例未终止 ${capacity.reserved-capacity.active} 个槽位待创建</span></div>`+
    panel('最近 200 个 Agent 池',fleets.length?table(['Agent 池 / 母体','策略','目标 / 当前','就绪 / 排空','状态','操作'],fleets.map(f=>`<tr><td><div class="primary-text">${esc(f.preset_name)} <span class="mono">@ ${esc(f.preset_version)}</span></div><div class="secondary-text mono">${esc(short(f.id))}</div></td><td>${f.mode==='maintain'?'持续维持':'有限批次'}</td><td class="mono">${f.desired_count} / ${f.current_count}</td><td class="mono">${f.ready_count} / ${f.draining_count}</td><td>${badge(f.state)}</td><td><a class="text-button" href="#fleet/${esc(f.id)}">查看成员</a></td></tr>`)):empty('还没有 Agent 池','点击“创建 Agent 池”，选择固定母体版本及有限批次或持续维持策略。'));
}
function fleetPage(f){
  const terminal=['draining','terminated'].includes(f.state),pending=f.pending_operations>0;
  const actions=(!terminal?button('调整目标数量','fleet-scale',true,`data-id="${esc(f.id)}" ${pending?'disabled':''}`)+button('终止 Agent 池','fleet-stop',false,`data-id="${esc(f.id)}"`):'');
  return '<a class="back-link" href="#fleets">← 全部 Agent 池</a>'+head(esc(f.preset_name)+' Agent 池', actions)+
    `<div class="goal-summary"><div><span>协调状态</span>${badge(f.state)}</div><div><span>目标 / 当前实例</span><strong>${f.desired_count} / ${f.current_count}</strong></div><div><span>就绪 / 执行 / 排空</span><strong>${f.ready_count} / ${f.busy_count} / ${f.draining_count}</strong></div></div>`+
    (pending?'<p role="status">操作协调中，完成后可调整数量。</p>':'')+
    panel('成员与补建记录',table(['位置','创建代次','是否补足','当前实例','状态','检查'],f.slots.map(s=>`<tr><td class="mono">${s.slot}</td><td class="mono">${s.generation}</td><td>${s.enabled?'纳入目标数量':'已移出'}</td><td class="mono wrap">${esc(s.instance_id||'等待创建')}</td><td>${s.state?badge(s.state):badge('pending','等待创建')}</td><td>${s.instance_id&&data.instances.some(a=>a.id===s.instance_id)?`<button class="text-button" data-action="agent-detail" data-id="${esc(s.instance_id)}">实例 ↗</button>`:'—'}</td></tr>`)))+
    panel('最近 20 次操作',f.operations.length?table(['操作 ID','动作','目标数 / 配置版本','状态','接纳时间','结算时间'],f.operations.map(o=>`<tr><td class="mono wrap">${esc(o.id)}</td><td>${esc({create:'创建',scale:'扩缩容',terminate:'终止'}[o.kind]||o.kind)}</td><td class="mono">${o.target_count} / ${o.revision}</td><td>${badge(o.state,o.state==='pending'?'等待协调':undefined)}</td><td>${time(o.created_at)}</td><td>${time(o.finished_at)}</td></tr>`)):empty('历史批次','由早期版本迁移的 Agent 池保留槽位与实例历史，没有伪造操作回执。'))+
    `<details class="raw-data" id="fleet-metadata"><summary>Agent 池身份与协调状态</summary>${identity('Fleet ID',f.id)}${dataView(f)}</details>`;
}
function fleetActionForm(id,stop){
  const f=selectedFleet?.id===id?selectedFleet:(data.fleets||[]).find(f=>f.id===id);if(!f)return;
  openModal(stop?'终止整个 Agent 池？':'调整目标实例数',`<form data-form="fleet-action" data-id="${esc(id)}" data-revision="${f.revision}" data-operation="${stop?'terminate':'scale'}"><p class="form-intro">${stop?'停止补建并排空所有成员。已接纳的任务结算后终止，历史会话与回执保留，Agent 池无法恢复。':'扩容会预留容量并分批创建；缩容按槽位号从大到小排空，已接纳的运行可继续收尾。可缩到零并保留 Agent 池。'}</p>${identity('Fleet ID',id)}${stop?'':`<label>目标实例数<input name="count" type="number" required min="0" max="${data.capacity?.limit||256}" step="1" value="${f.desired_count}"></label>`}${formEnd(stop?'确认终止 Agent 池':'提交扩缩容')}</form>`,'Agent 池控制');
  if(stop)$('#modal').setAttribute('role','alertdialog');
}
