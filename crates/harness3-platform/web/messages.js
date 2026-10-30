'use strict';
const messageLabels={pending:'等待',delivered:'已入 Inbox',undeliverable:'无法投递',discarded:'已丢弃',queued:'已派发 排队',running:'消费中',consumed:'消费完成',failed:'消费失败',cancelled:'已取消',unknown:'结果未知'};
function messageBadge(state) {return state?badge(state,messageLabels[state]||state):'<span class="secondary-text">尚未入 Inbox</span>';}
function messagesPage() {
  const list=data.messages||[];
  return head('协作消息', button('发送消息','message-form',true))+
    panel('最近 100 条消息',list.length?table(['内容 / 消息','发送方 → 接收方','投递','消费','创建时间','操作'],list.map(m=>`<tr><td class="message-preview"><div class="primary-text">${esc(m.text_preview)}</div><div class="secondary-text mono">${esc(short(m.id))}</div></td><td><div class="secondary-text ${m.sender_instance_id?'mono':''}">${esc(m.sender_instance_id?short(m.sender_instance_id):'本地操作者')} →</div><div class="mono">${esc(short(m.recipient_id))}</div></td><td>${messageBadge(m.delivery_state)}</td><td>${messageBadge(m.inbox_state)}${m.inbox_state==='unknown'?`<div class="secondary-text">${m.resolved_at?'已记录人工处置':'等待人工处置 阻止后续派发'}</div>`:''}</td><td>${time(m.created_at)}</td><td><button class="text-button" data-action="message-detail" data-id="${esc(m.id)}">检查</button></td></tr>`)):empty('尚无协作消息','向已有实例发送任务数据；就绪后平台会自动创建 Run。暂停或忙碌实例保留待消费消息。'));
}
function messageForm() {
  const recipients=data.instances.filter(a=>!['terminated','draining'].includes(a.state));
  if(!recipients.length)return toast('请先创建可接收消息的实例。');
  openModal('发送协作消息',`<form data-form="message-send"><label>接收实例<select name="recipient_id">${recipients.map(a=>`<option value="${esc(a.id)}">${esc(a.preset_name)} ${esc(a.id)} ${esc(labels[a.state])}</option>`).join('')}</select></label><label>任务数据<textarea name="text" required maxlength="8000" rows="6" placeholder="需要接收实例处理的具体内容"></textarea><span class="hint">最多 8,000 个 UTF-8 字节。消息不能授予权限或覆盖母体指令。</span></label><details><summary>可选关联上下文</summary><label>协作目标 ID<input name="goal_id" pattern="[A-Za-z0-9._-]+"><span class="hint">接收者必须属于正在运行或待验收的该目标。</span></label><label>关联 ID<input name="correlation_id" pattern="[A-Za-z0-9._-]+"></label><label>前置消息 ID<input name="causation_id" pattern="[A-Za-z0-9._-]+"><span class="hint">填写后继承前置消息的关联、目标与深度。</span></label></details><p class="callout">接收者就绪后自动派发新的 Run，可能产生模型费用。暂停或执行中的实例将等待；投递成功不代表任务完成。</p>${formEnd('接纳并等待派发')}</form>`);
}
async function messageDetail(id) {
  openModal('消息详情','<div class="loading">正在读取消息与消费状态…</div>');
  const ticket=inspectionRequest;
  const m=await api(`/api/v1/messages/${encodeURIComponent(id)}`);
  if(ticket!==inspectionRequest||!$('#modal').open)return;
  const pending=m.delivery_state==='pending'||m.inbox_state==='pending',unknown=m.inbox_state==='unknown'&&!m.resolved_at;
  $('#modal').classList.add('inspector');
  $('#modal-body').innerHTML=`<div class="inspector-summary"><div><span>投递</span>${messageBadge(m.delivery_state)}</div><div><span>消费</span>${messageBadge(m.inbox_state)}</div><div><span>因果深度</span><strong>${m.depth} / 8</strong></div></div><div class="form-actions">${button('刷新详情','message-detail',false,`data-id="${esc(id)}"`)}${m.run_id?button('检查消费 Run','run-detail',false,`data-id="${esc(m.run_id)}"`):''}${pending?button('丢弃待处理消息','message-discard',false,`data-id="${esc(id)}"`):''}${unknown?button('记录人工处置','message-resolve',false,`data-id="${esc(id)}"`):''}</div>${unknown?'<p class="callout">执行效果未知，后续 Inbox 派发已暂停。核对 Run 与工具回执后记录处置；平台不会自动重试。</p>':''}${identity('消息 ID',id)}<dl class="detail-kv"><dt>发送身份</dt><dd>${esc(m.sender_kind)} ${esc(m.sender_instance_id||'本地操作者')}</dd><dt>来源 Run</dt><dd class="mono wrap">${esc(m.source_run_id||'—')}</dd><dt>接收实例</dt><dd class="mono wrap">${esc(m.recipient_id)}</dd><dt>目标 / 关联</dt><dd class="mono wrap">${esc(m.goal_id||'—')}<br>${esc(m.correlation_id)}</dd><dt>前置消息</dt><dd class="mono wrap">${esc(m.causation_id||'—')}</dd><dt>入 Inbox / 派发</dt><dd>${time(m.received_at)} / ${time(m.ack_at)}</dd><dt>消费完成</dt><dd>${time(m.finished_at)}</dd><dt>错误</dt><dd>${esc(m.error||m.delivery_error||'—')}</dd><dt>处置记录</dt><dd class="instructions">${esc(m.resolution||'—')}</dd></dl><section class="panel"><div class="panel-head">任务数据</div><pre class="message-body">${esc(m.text)}</pre></section>${rawView(m,'完整消息封套与状态')}`;
}
async function messageAction(action,element) {
  if(action==='message-form'){messageForm();return true;}
  if(action==='message-detail'){await messageDetail(element.dataset.id);return true;}
  if(!['message-discard','message-resolve'].includes(action))return false;
  const resolve=action==='message-resolve';
  openModal(resolve?'记录未知结果处置':'丢弃待处理消息',`<form data-form="message-action" data-id="${esc(element.dataset.id)}" data-operation="${resolve?'resolve':'discard'}"><p class="form-intro">${resolve?'请先核对执行及外部效果。保存处置后允许该实例继续消费后续消息；原消息保留“结果未知”，不会重试或撤销效果。':'只允许丢弃尚未派发的消息，已有运行无法通过此操作取消。若派发已经发生，操作会被拒绝。'}</p><label>处置说明<textarea name="note" required maxlength="2000" rows="4"></textarea></label>${formEnd(resolve?'保存处置并解除阻塞':'确认丢弃')}</form>`);return true;
}
