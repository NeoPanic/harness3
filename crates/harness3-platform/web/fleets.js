'use strict';
let selectedFleet=null;
function fleetId(){return location.hash.startsWith('#fleet/')?location.hash.slice(7):null;}
function fleetsPage(){
  const id=fleetId();
  if(id)return selectedFleet?.id===id?selectedFleet.loadError?`<a class="back-link" href="#fleets">← 全部实例组</a><div class="callout" role="alert">${esc(selectedFleet.loadError)}</div>`:fleetPage(selectedFleet):'<div class="loading">正在读取实例组与操作…</div>';
  const fleets=data.fleets||[],capacity=data.capacity;
  return head('FLEET CONTROL','实例组','有限批次与持续维持共用持久槽位。调整目标数量，检查排空、补足与操作进度。',button('＋ 创建实例组','spawn',true))+
    `<div class="callout">全局容量 ${capacity.reserved} / ${capacity.limit} 已占用或预留；其中 ${capacity.active} 个实例尚未终止，${capacity.reserved-capacity.active} 个槽位等待创建。每轮最多创建 8 个实例，不占用模型 Worker。</div>`+
    panel('最近 200 个实例组',fleets.length?table(['实例组 / 母体','策略','目标 / 当前','就绪 / 排空','状态','操作'],fleets.map(f=>`<tr><td><div class="primary-text">${esc(f.preset_name)} <span class="mono">@ ${esc(f.preset_version)}</span></div><div class="secondary-text mono">${esc(short(f.id))}</div></td><td>${f.mode==='maintain'?'持续维持':'有限批次'}</td><td class="mono">${f.desired_count} / ${f.current_count}</td><td class="mono">${f.ready_count} / ${f.draining_count}</td><td>${badge(f.state)}</td><td><a class="text-button" href="#fleet/${esc(f.id)}">检查 ↗</a></td></tr>`)):empty('还没有实例组','从一个固定版本的母体创建有限批次或持续维持的实例组。',button('创建实例组','spawn',true)));
}
function fleetPage(f){
  const terminal=['draining','terminated'].includes(f.state),pending=f.pending_operations>0;
  const actions=(!terminal?button('调整目标数量','fleet-scale',true,`data-id="${esc(f.id)}" ${pending?'disabled':''}`)+button('终止实例组','fleet-stop',false,`data-id="${esc(f.id)}"`):'');
  return '<a class="back-link" href="#fleets">← 全部实例组</a>'+head('MANAGED FLEET',esc(f.preset_name),`${f.mode==='maintain'?'持续维持':'有限批次'} · 母体 ${esc(f.preset_version)} · revision ${f.revision}`,actions)+
    `<div class="goal-summary"><div><span>协调状态</span>${badge(f.state)}</div><div><span>目标 / 当前实例</span><strong>${f.desired_count} / ${f.current_count}</strong></div><div><span>就绪 / 执行 / 排空</span><strong>${f.ready_count} / ${f.busy_count} / ${f.draining_count}</strong></div></div>`+
    `<div class="callout">${terminal?'此组已停止补建，排空结束后保留终态与全部历史，无法恢复。':f.mode==='maintain'?'单独终止或排空组内实例后，平台会使用新的身份与会话补足同一槽位；每次补建递增 generation。停止补建请终止整个实例组或缩容。':'实例终止或 TTL 到期后不自动补建。显式扩缩容会按本次目标填足启用槽位；之后仍不自动补建。旧槽位再次创建成员时递增 generation。'} ${f.ttl_seconds?`每个新实例 TTL 为 ${f.ttl_seconds} 秒，从该实例创建时计时。`:'新实例未设置 TTL；历史成员以各自截止时间为准。'} ${pending?'当前操作正在协调，完成前暂不能再次扩缩容；终止可以中止补建并替代未完成操作。':''}</div>`+
    panel('持久槽位',table(['槽位','代际','意图','当前实例','观测状态','检查'],f.slots.map(s=>`<tr><td class="mono">${s.slot}</td><td class="mono">${s.generation}</td><td>${s.enabled?'启用':'退休'}</td><td class="mono wrap">${esc(s.instance_id||'等待创建')}</td><td>${s.state?badge(s.state):badge('pending','等待创建')}</td><td>${s.instance_id&&data.instances.some(a=>a.id===s.instance_id)?`<button class="text-button" data-action="agent-detail" data-id="${esc(s.instance_id)}">实例 ↗</button>`:'—'}</td></tr>`)))+
    panel('最近 20 次操作',f.operations.length?table(['Operation','动作','目标数 / revision','状态','接纳时间','结算时间'],f.operations.map(o=>`<tr><td class="mono wrap">${esc(o.id)}</td><td>${esc({create:'创建',scale:'扩缩容',terminate:'终止'}[o.kind]||o.kind)}</td><td class="mono">${o.target_count} / ${o.revision}</td><td>${badge(o.state,o.state==='pending'?'等待协调':undefined)}</td><td>${time(o.created_at)}</td><td>${time(o.finished_at)}</td></tr>`)):empty('历史批次','由早期版本迁移的实例组保留槽位与实例历史，没有伪造操作回执。'))+
    `<details class="raw-data" id="fleet-metadata"><summary>实例组身份与协调状态</summary>${identity('Fleet ID',f.id)}${dataView(f)}</details>`;
}
function fleetActionForm(id,stop){
  const f=selectedFleet?.id===id?selectedFleet:(data.fleets||[]).find(f=>f.id===id);if(!f)return;
  openModal(stop?'终止整个实例组？':'调整目标实例数',`<form data-form="fleet-action" data-id="${esc(id)}" data-revision="${f.revision}" data-operation="${stop?'terminate':'scale'}"><p class="form-intro">${stop?'停止补建并排空所有成员。已接纳的任务结算后终止，历史会话与回执保留，实例组无法恢复。':'扩容会预留容量并分批创建；缩容按槽位号从大到小排空，已接纳的运行可继续收尾。可缩到零并保留实例组。'}</p>${identity('Fleet ID',id)}${stop?'':`<label>目标实例数<input name="count" type="number" required min="0" max="64" step="1" value="${f.desired_count}"></label>`}<p class="hint">基于 revision ${f.revision} 提交；若其他入口已修改配置，会拒绝覆盖并要求重新检查。</p>${formEnd(stop?'确认终止实例组':'提交扩缩容')}</form>`,'实例组控制');
  if(stop)$('#modal').setAttribute('role','alertdialog');
}
