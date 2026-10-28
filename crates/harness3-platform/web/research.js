'use strict';
let selectedResearch=null;
const researchLabels={provisioning:'创建团队',planning:'规划分工',running:'研究与汇总',cancelling:'取消收尾','awaiting-review':'待验收',accepted:'已验收',failed:'失败',cancelled:'已取消'};
function researchId(){return location.hash.startsWith('#research/')?location.hash.slice(10):null;}
function researchPage(){
 const id=researchId();if(id)return selectedResearch?.id===id?selectedResearch.loadError?`<a class="back-link" href="#research">← 全部调研任务</a><div class="callout">${esc(selectedResearch.loadError)}</div>`:researchDetail(selectedResearch):'<div class="loading">正在读取团队与任务进度…</div>';
 const jobs=data.research_jobs||[];
 return head('','自主调研','提交一个目标，由规划 Agent 拆分任务，多个独立实例并行研究，再由汇总 Agent 整合贡献。',button('发起调研任务','research-form',true))+
 `<p class="callout">每项调研派生 1 个规划 Agent、2–6 个研究 Agent 和 1 个汇总 Agent。研究能力来自所选母体的插件；平台不会自动增加联网检索能力。</p>`+
 panel('调研任务',jobs.length?table(['目标','研究实例','阶段','创建时间','操作'],jobs.map(j=>`<tr><td><div class="primary-text wrap">${esc(j.title)}</div><div class="secondary-text mono">${esc(short(j.id))}</div></td><td>${j.researchers} + 规划 / 汇总</td><td>${badge(j.state,researchLabels[j.state]||j.state)}</td><td>${time(j.created_at)}</td><td><a class="text-button" href="#research/${esc(j.id)}">查看团队 ↗</a></td></tr>`)):empty('发起第一项协作调研','无需手工创建实例或逐项填写任务图，平台管理分工、运行和回收。'));
}
function researchDetail(j){
 const goal=j.goal,members=j.members||[],active=['provisioning','planning','running','cancelling'].includes(j.state),tasks=goal?.tasks||[],synthesis=tasks.find(t=>t.key==='synthesis');
 const role={planner:'规划 Agent',researcher:'研究 Agent',synthesizer:'汇总 Agent'};
 return `<a class="back-link" href="#research">← 全部调研任务</a>`+head('',esc(j.title),'规划、独立研究、证据汇总与人工验收使用不同实例；各阶段状态来自持久运行。',(['provisioning','planning','running'].includes(j.state)?button('取消调研','research-cancel',false,`data-id="${esc(j.id)}"`):'')+(goal?.state==='awaiting-review'&&goal.round<3?button('要求修改','goal-rework',false,`data-id="${esc(goal.id)}"`):'')+(goal?.state==='awaiting-review'?button('验收结果','goal-review',true,`data-id="${esc(goal.id)}"`):''))+
 `<div class="goal-summary"><div><span>当前阶段</span>${badge(j.state,researchLabels[j.state]||j.state)}</div><div><span>团队规模</span><strong>${j.researchers} 位研究者 + 规划 / 汇总</strong></div><div><span>任务进度</span><strong>${goal?`第 ${goal.round} 轮 · `:''}${tasks.filter(t=>t.state==='completed').length} / ${tasks.length||'待规划'}</strong></div></div>`+
 `${j.error?`<div class="callout">${esc(j.error)}。检查规划 Run、模型配置与团队状态；失败任务不会自动重试。</div>`:''}`+
 panel('调研目标',`<div class="panel-body"><dl class="definition"><dt>目标</dt><dd>${esc(j.objective)}</dd><dt>验收标准</dt><dd>${esc(j.acceptance)}</dd><dt>运行上限</dt><dd>15 分钟；不递归派生。${active?'状态每 1.5 秒刷新。':'团队已请求回收，历史证据保留。'}</dd></dl></div>`)+
 panel('团队与派生关系（含历史轮次）',members.length?table(['角色','实例','父级身份','状态','操作'],members.map(m=>`<tr><td>${role[m.role]}<div class="secondary-text">${m.role==='planner'?'初始规划':`第 ${goal?.tasks.some(t=>t.instance_id===m.instance_id)?goal.round:goal?.rounds.find(r=>r.tasks?.some(t=>t.instance_id===m.instance_id))?.number||1} 轮`}</div></td><td class="mono">${esc(short(m.instance_id))}</td><td class="mono">${esc(m.parent_instance_id?short(m.parent_instance_id):'任务根节点')}</td><td>${badge(m.state)}</td><td><button class="text-button" data-action="agent-detail" data-id="${esc(m.instance_id)}">检查</button></td></tr>`)):empty('正在创建团队','容量已预留，实例将由控制面创建。'),button('初始实例组','fleet-nav',false,`data-id="${esc(j.fleet_id)}"`))+
 panel('初始规划结果',j.plan?`<div class="panel-body"><ol class="research-plan">${j.plan.tasks.map(t=>`<li><strong>${esc(t.title)}</strong><p>${esc(t.instructions)}</p></li>`).join('')}</ol><p><strong>汇总要求</strong></p><p class="instructions">${esc(j.plan.synthesis)}</p></div>`:empty('等待分工计划','规划 Agent 只能提交分工，无法在规划阶段调用研究工具。'),j.planner_run_id?button('检查规划 Run','run-detail',false,`data-id="${esc(j.planner_run_id)}"`):'')+
 (synthesis?.run?.report?.output?panel('汇总结果',`<div class="panel-body output">${esc(synthesis.run.report.output)}</div>`,button('检查汇总 Run','run-detail',false,`data-id="${esc(synthesis.run_id)}"`)):'')+
 (goal?goalHistory(goal)+`<div class="goal-task-heading"><h2>并行研究与贡献传递</h2><a class="text-button" href="#goal/${esc(goal.id)}">检查依赖图 ↗</a></div><div class="work-items">${tasks.map((t,i)=>workItem(goal,{...t,display_title:t.key==='synthesis'?'汇总与对照':j.plan?.tasks[i]?.title||t.key,instructions:t.key==='synthesis'?j.plan?.synthesis||t.instructions:j.plan?.tasks[i]?.instructions||t.instructions})).join('')}</div>`:'');
}
function researchForm(){
 if(!data.presets.length)return toast('请先发布可用于研究的母体。');
 const preferred=data.presets.find(p=>p.definition.protocol==='openai-responses')||data.presets[0];
 openModal('发起自主调研',`<form data-form="research"><label>任务名称<input name="title" required maxlength="200" placeholder="例如：比较三种 Agent 协作架构"></label><label>调研目标<textarea name="objective" required maxlength="8000" rows="4" placeholder="描述需要研究的问题、已有资料及关注范围。平台将自动拆分为互补的研究任务。"></textarea></label><label>验收标准<textarea name="acceptance" required maxlength="8000" rows="3" placeholder="例如：列出方案差异、引用证据、说明限制与未核实结论。"></textarea></label><label>团队母体<select name="preset">${data.presets.map(p=>`<option value="${esc(JSON.stringify({name:p.name,version:p.version}))}" ${p===preferred?'selected':''}>${esc(p.name)} @ ${esc(p.version)} · ${esc(p.definition.protocol)}</option>`).join('')}</select></label><div id="research-capabilities" class="callout"></div><div id="spawn-parameters">${spawnParameterFields(preferred.definition)}</div><label>并行研究者数量<select name="researchers">${[2,3,4,5,6].map(n=>`<option value="${n}" ${n===3?'selected':''}>${n} 位研究者 + 1 位规划 + 1 位汇总，共 ${n+2} 个实例</option>`).join('')}</select></label><p class="hint">同一固定母体派生独立身份和会话，规划阶段只允许提交分工。每位研究者接收不同任务，汇总等待所有贡献。任务结束后回收实例，保留历史；不递归扩张。默认 15 分钟期限，可能产生多次模型费用，当前没有费用预算预留。</p>${formEnd('创建团队并开始规划')}</form>`);
 updateResearchCapabilities($('#modal form'));
}
function updateResearchCapabilities(form){const ref=JSON.parse(form.elements.preset.value),p=data.presets.find(p=>p.name===ref.name&&p.version===ref.version);const tools=['text.inspect',...presetBindings(p.definition).flatMap(b=>b.kind==='process'?b.manifest.tools.map(t=>`${b.id}.${t.name}`):b.kind==='organization'?['platform.messaging.send']:[])];$('#research-capabilities').textContent=`${p.definition.protocol==='demo'?'确定性演示，只验证多实例执行链路，不执行语义调研。':'使用该母体的模型；检索行为取决于实际绑定的工具。'} 当前工具：${tools.join('、')}。未配置检索工具时，只能分析输入资料或模型已有知识。`;}
async function researchAction(action,element){
 if(action==='research-form'){researchForm();return true;}
 if(action!=='research-cancel')return false;
 openModal('取消调研并回收团队？',`<form data-form="research-cancel" data-id="${esc(element.dataset.id)}"><p class="form-intro">停止规划和后续派发，取消排队任务，请求活动运行协作停止。已有工具效果不撤销，全部运行与贡献保留。</p>${formEnd('确认取消调研')}</form>`);return true;
}
document.addEventListener('change',event=>{const form=event.target.closest('form[data-form=research]');if(form&&event.target.name==='preset'){refreshSpawnParameters(form);updateResearchCapabilities(form);}});
