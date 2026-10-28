'use strict';
let selectedGoal = null;
function goalId() { return location.hash.startsWith('#goal/') ? location.hash.slice(6) : null; }
function goalsPage() {
  const id=goalId();
  if(id) return selectedGoal?.id===id ? selectedGoal.loadError?`<a class="back-link" href="#goals">← 全部协作目标</a><div class="callout" role="alert">${esc(selectedGoal.loadError)} 页面将自动重试。</div>`:goalPage(selectedGoal) : '<div class="loading">正在读取目标与任务证据…</div>';
  const goals=data.goals||[];
  return head('SHARED GOALS','协作目标','显式分工、依赖调度与结果传递；任务完成后由你核对验收标准。',button('＋ 创建目标','goal-form',true))+
    '<div class="callout">独立任务可并行执行，依赖任务接收上游输出及 Run ID。同一实例按队列串行执行；暂停的实例会等待恢复。任务可授权动态委派；自动规划和创建初始团队请使用“自主调研”；点对点交接见“协作消息”。</div>'+
    panel('最近目标',goals.length?table(['目标 / GOAL','任务进度','状态','创建时间','操作'],goals.map(g=>`<tr><td><div class="primary-text wrap">${esc(g.title)}</div><div class="secondary-text mono">${esc(short(g.id))}</div></td><td>${g.completed_count} / ${g.task_count}</td><td>${badge(g.state)}</td><td>${time(g.created_at)}</td><td><a class="text-button" href="#goal/${esc(g.id)}">查看目标 ↗</a></td></tr>`)):empty('还没有协作目标','为已有实例分配任务，并指定哪些任务需要等待上游结果。'),badge('neutral',`${goals.length} 条`));
}
function goalPage(g) {
  const active=['running','verifying','cancelling'].includes(g.state),actions=(g.state==='awaiting-review'&&g.round<3?button('要求修改','goal-rework',false,`data-id="${esc(g.id)}"`):'')+(g.state==='awaiting-review'&&verificationReady(g)?button('验收目标','goal-review',true,`data-id="${esc(g.id)}"`):'')+(['running','verifying','awaiting-review'].includes(g.state)?button('取消目标','goal-cancel',false,`data-id="${esc(g.id)}"`):'');
  const complete=g.tasks.filter(t=>t.state==='completed').length;
  return `<a class="back-link" href="#goals">← 全部协作目标</a>`+head('SHARED GOAL',esc(g.title),'任务成功只表示执行已结算，目标是否达成需要按验收标准核对。',actions)+
    `<div class="goal-summary"><div><span>目标状态</span>${badge(g.state)}</div><div><span>任务完成</span><strong>${complete} / ${g.tasks.length}</strong></div><div><span>同步方式</span><strong>${active?'每 1.5 秒同步':'已结算 · 持久保存'}</strong></div></div>`+
    panel('共同目标与验收',`<div class="panel-body"><dl class="definition"><dt>共同目标</dt><dd>${esc(g.objective)}</dd><dt>验收标准</dt><dd>${esc(g.acceptance)}</dd>${g.review?`<dt>验收记录</dt><dd>${esc(g.review)}</dd>`:''}</dl><details class="raw-data" id="goal-identity"><summary>目标身份与计划</summary>${identity('Goal ID',g.id)}${dataView({title:g.title,objective:g.objective,acceptance:g.acceptance,tasks:g.tasks.map(({key,instance_id,instructions,depends_on,claim})=>({key,instance_id,instructions,depends_on,claim}))})}</details></div>`)+
    verificationPanel(g)+delegationOverview(g)+goalContextPanel(g)+`<div class="goal-task-heading"><h2>任务与贡献</h2><span class="hint">${active?'状态自动同步；运行详情按需刷新。':'所有结果保留原始运行链接。'}</span></div><div class="work-items">${g.tasks.map(t=>workItem(g,t)).join('')}</div>`+goalHistory(g);
}
function workItem(g,t) {
  const a=data.instances.find(a=>a.id===t.instance_id),error=t.error||t.run?.report?.error?.code;
  const messages={dependency_failed:'上游任务未成功，已阻断后续执行。',instance_terminated:'指定实例已终止，无法派发。',dependency_context_too_large:'目标与上游输出合计超过 64,000 UTF-8 字节，未截断或派发。'};
  const pending=t.state==='pending'?(t.depends_on.some(key=>g.tasks.find(item=>item.key===key)?.state!=='completed')?'等待依赖任务完成':t.claim?.mode==='manual'?'等待人工认领':t.claim?.candidates.length>1?'等待候选实例可用':a?.state==='suspended'?'实例已暂停，等待恢复':a?.state==='busy'?'等待实例结束当前运行':'等待 Worker 派发'):'';
  return `<section class="work-item" aria-labelledby="task-heading-${esc(t.key)}"><div class="work-item-head"><div><h3 id="task-heading-${esc(t.key)}">${esc(t.display_title||t.key)}</h3>${badge(t.state)}</div>${taskClaimButtons(g,t)}${t.artifact?.version<g.round?`<span class="badge neutral">沿用第 ${t.artifact.version} 轮</span>`:''}${artifactLink(t.artifact)}${t.run_id?button('检查运行 ↗','run-detail',false,`data-id="${esc(t.run_id)}"`):''}</div><div class="work-item-meta"><span>执行实例 <span class="mono">${esc(a?.preset_name||'Agent')} · ${esc(short(t.instance_id))}</span></span><span>候选 ${t.claim?.candidates.length||1} 位 · ${t.claim?.mode==='manual'?'人工认领':'自动分配'}</span><span>依赖 ${t.depends_on.length?t.depends_on.map(key=>`<a href="#task-heading-${esc(key)}" data-task-anchor="${esc(key)}">${esc(g.tasks.find(x=>x.key===key)?.display_title||key)}</a>`).join('、'):'无 · 可独立执行'}</span></div><p class="task-instructions">${esc(t.instructions)}</p>${pending?`<p class="hint">${pending}</p>`:''}${error?`<div class="callout">${esc(messages[error]||error)}</div>`:''}${t.run?.report?.output?`<details class="task-output" id="output-${esc(t.key)}"><summary>${t.delegations?.some(d=>d.source_run_id===t.run_id)?'查看规划交接':'查看贡献输出'} <span class="mono">${esc(short(t.run_id))}</span></summary><div class="output">${esc(t.run.report.output)}</div></details>`:''}${taskDelegation(g,t)}${taskAttempts(t)}</section>`;
}
function taskFields(index) {
  const agents=data.instances.filter(a=>a.state!=='terminated');
  return `<fieldset class="goal-task-fields" data-task-index="${index}"><legend>task-${index+1}</legend><label>执行实例<select name="task_agent_${index}" required>${agents.map((a,i)=>`<option value="${esc(a.id)}" ${i===index%agents.length?'selected':''}>${esc(a.preset_name)} · ${esc(short(a.id))} · ${labels[a.state]} · ${esc(a.snapshot.protocol)}</option>`).join('')}</select></label><label>任务指令<textarea name="task_text_${index}" required maxlength="8000" rows="2" placeholder="描述这项任务的职责与输出要求"></textarea></label>${taskPoolFields(index,agents)}${delegationFields(`task-${index}`)}${index?`<div class="dependency-picker"><span>依赖任务 <small>勾选后，该任务将接收上游输出。</small></span><div>${Array.from({length:index},(_,i)=>`<label><input type="checkbox" name="task_deps_${index}" value="task-${i+1}">task-${i+1}</label>`).join('')}</div></div>`:'<p class="hint">无上游依赖，可以独立执行。</p>'}</fieldset>`;
}
function goalForm() {
  if(!data.instances.some(a=>a.state!=='terminated')) {toast('请先从母体创建至少一个实例。');location.hash='agents';return;}
  openModal('创建协作目标',`<form data-form="goal"><p class="form-intro">指定共同目标、验收标准与任务分工。依赖结果会传递给下游实例；真实模型按各自配置计费。复用实例会保留既有会话，需要隔离历史时请先创建新实例。</p><label>目标名称<input name="title" required maxlength="200" placeholder="例如：资料分析与证据汇总"></label><div class="grid-two"><label>共同目标<textarea name="objective" required maxlength="8000" rows="3" placeholder="这组 Agent 需要共同达成什么？"></textarea></label><label>验收标准<textarea name="acceptance" required maxlength="8000" rows="3" placeholder="你将根据什么证据确认完成？"></textarea></label></div><div id="goal-task-fields">${taskFields(0)}</div><div class="task-form-actions">${button('＋ 添加任务','goal-add-task')}${button('移除最后一项','goal-remove-task',false,'disabled')}<span class="hint">最多 16 项。任务可以复用同一实例。</span></div>${verificationFields()}${formEnd('启动目标')}</form>`,'持久化任务计划');
  $('#modal').classList.add('inspector');
}
async function goalReview(id) {
  const g=await api(`/api/v1/goals/${encodeURIComponent(id)}`);
  openModal('验收目标',`<form data-form="goal-review" data-id="${esc(id)}" data-round="${g.round}" data-context-revision="${g.shared_context?.revision||0}"><p class="form-intro">请核对每项贡献与目标验收标准。确认后固定第 ${g.round} 轮产物、内容摘要和共享上下文 v${g.shared_context?.revision||0}，记录人工验收结论。</p><label>验收结论<textarea name="review" required maxlength="8000" rows="4" placeholder="记录核对的证据与结论"></textarea></label>${formEnd('确认验收')}</form>`,'人工验收');
}
function goalCancel(id) {
  openModal('取消这个目标？',`<form data-form="goal-cancel" data-id="${esc(id)}"><p class="form-intro">停止派发后续任务，取消排队工作，并请求正在执行的运行停止。已产生的贡献和回执会保留；取消不会撤销已有结果。</p>${formEnd('确认取消目标')}</form>`,'目标取消');
  $('#modal').setAttribute('role','alertdialog');
}
function goalPlanFromForm(form) {
  const fields=new FormData(form);
  const plan={verification:verificationFromForm(form),title:fields.get('title'),objective:fields.get('objective'),acceptance:fields.get('acceptance'),tasks:[...form.querySelectorAll('[data-task-index]')].map((el,index)=>({key:`task-${index+1}`,instance_id:fields.get(`task_agent_${index}`),instructions:fields.get(`task_text_${index}`),depends_on:fields.getAll(`task_deps_${index}`),candidates:fields.getAll(`task_candidates_${index}`).filter(id=>id!==fields.get(`task_agent_${index}`)),manual_claim:fields.has(`task_manual_${index}`),delegation:delegationFromFields(el.querySelector('[data-delegation-config]'))}))};
  for(const task of plan.tasks)if(task.delegation)for(const id of [task.instance_id,...task.candidates])validateDelegationBinding(data.instances.find(a=>a.id===id)?.snapshot||{});
  if(plan.tasks.some(t=>t.candidates.length>15))throw new Error('每项任务最多选择 15 个备用候选实例。');
  for(const [field,limit] of [['title',200],['objective',8000],['acceptance',8000],...plan.tasks.map((t,i)=>[`task_text_${i}`,8000])]) {
    const el=form.elements.namedItem(field);
    if(new TextEncoder().encode(el.value).length>limit) {el.focus();throw new Error(`内容过长：${el.closest('label').firstChild.textContent.trim()}最多 ${limit.toLocaleString()} 个 UTF-8 字节。`);}
  }
  return plan;
}
