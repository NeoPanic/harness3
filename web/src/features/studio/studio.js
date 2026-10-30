'use strict';

const studio={view:'network',selected:null,selectedTask:null,boardRootId:'',goalId:'',goal:null,goalError:'',paused:null,page:0,scope:'live',syncedAt:null,disconnected:false};
const studioStates={busy:'工作中',running:'工作中',ready:'待命',queued:'排队',pending:'等待',staged:'待结算',waiting:'等待子任务',suspended:'休眠',draining:'收尾',terminated:'离场',completed:'完成',failed:'遇到问题',blocked:'上游阻断',interrupted:'结果未知',cancelled:'取消',delegating:'委派中'};
const studioSpots=[{x:130,y:270},{x:360,y:210},{x:710,y:210},{x:960,y:275},{x:1040,y:465},{x:830,y:605},{x:535,y:625},{x:250,y:600},{x:50,y:465}];

async function studioReadGoal(){
  if(studio.paused)return;
  if(!studio.goalId)studio.goalId=(data.goals.find(g=>['running','verifying'].includes(g.state))||data.goals[0])?.id||'';
  if(!studio.goalId){studio.goal=null;return;}
  const id=studio.goalId;
  try{const goal=await api(`/api/v1/goals/${encodeURIComponent(id)}`);if(studio.goalId===id){studio.goal=goal;studio.goalError='';}}
  catch(error){if(studio.goalId===id)studio.goalError=error.message;}
}

function studioSyncLabel(){
  const el=$('#studio-sync');
  if(el){
    el.textContent=studio.disconnected?'离线':studio.paused?'已暂停':'实时';
    el.title=studio.disconnected?`连接中断，保留 ${time(studio.syncedAt)} 的画面`:`${time(studio.paused?.at||studio.syncedAt)} 每 1.5 秒同步`;
    el.classList.toggle('is-offline',studio.disconnected);
  }
}

function studioPerson(state='ready',seed='',phase=''){
  const working=['busy','running'].includes(state),sleeping=['suspended','terminated'].includes(state),issue=['failed','blocked','interrupted'].includes(state),done=state==='completed';
  const variant=[...seed].reduce((n,c)=>n+c.charCodeAt(0),0)%4;
  const ears=variant===1?'<path class="creature-fur" d="M64 69C43 22 49 5 61 9c11 3 13 30 14 50M109 59c-1-38 9-58 20-50 10 8-1 41-10 60"/><path class="creature-ear" d="m61 25 8 30m54-30-9 31"/>':variant===2?'<circle class="creature-fur" cx="57" cy="68" r="20"/><circle class="creature-fur" cx="126" cy="68" r="20"/><circle class="creature-ear" cx="57" cy="68" r="10"/><circle class="creature-ear" cx="126" cy="68" r="10"/>':'<path class="creature-fur" d="M51 85 45 40q1-7 7-3l31 23m20-1 29-23q6-4 7 3l-5 47"/><path class="creature-ear" d="m55 63-2-14 17 13m45-1 15-12-3 16"/>';
  const eyes=sleeping?'<path class="creature-eye-line" d="m65 97 10 3m29 0 10-3"/>':done?'<path class="creature-eye-line" d="m64 100q6-12 12 0m26 0q6-12 12 0"/>':issue?'<ellipse class="creature-eye" cx="70" cy="98" rx="6" ry="9"/><ellipse class="creature-eye" cx="109" cy="98" rx="6" ry="9"/><path class="creature-eye-line" d="m61 84 14 4m29 0 14-4"/>':'<g class="creature-eyes"><ellipse class="creature-eye" cx="70" cy="98" rx="7" ry="10"/><ellipse class="creature-eye" cx="109" cy="98" rx="7" ry="10"/><circle class="creature-eye-shine" cx="72" cy="94" r="2.5"/><circle class="creature-eye-shine" cx="111" cy="94" r="2.5"/></g>';
  return `<svg class="studio-person creature-${variant} ${working?'is-working':sleeping?'is-sleeping':issue?'is-troubled':'is-idle'} ${phase.includes('工具')?'is-tooling':''}" viewBox="0 0 220 220" aria-hidden="true" fill="none" stroke-linecap="round" stroke-linejoin="round">
    <ellipse class="pod-shadow" cx="110" cy="202" rx="89" ry="14"/><path class="pod-base" d="M20 184v10c0 28 180 28 180 0v-10"/><ellipse class="pod-top" cx="110" cy="184" rx="90" ry="24"/><path class="pod-light" d="M33 199q77 24 154 0"/>
    <path class="creature-backpack" d="M43 134q-17 0-17 13v20q0 12 16 12h16v-45Z"/>
    <path class="creature-suit" d="M50 147q0-29 38-29t39 29v29H50Z"/><ellipse class="creature-foot" cx="63" cy="177" rx="19" ry="11"/><ellipse class="creature-foot" cx="115" cy="177" rx="19" ry="11"/>
    <g class="creature-head">${ears}<path class="creature-fur" d="M42 92c0-30 21-42 48-42s48 12 48 42v18c0 24-21 34-48 34s-48-10-48-34Z"/><path class="creature-face" d="M51 91c0-18 16-25 39-25s39 7 39 25v18c0 17-17 25-39 25s-39-8-39-25Z"/>${eyes}<ellipse class="creature-blush" cx="58" cy="112" rx="8" ry="4"/><ellipse class="creature-blush" cx="122" cy="112" rx="8" ry="4"/>
    ${issue?'<path class="creature-mouth" d="M83 121q7-9 14 0"/>':'<path class="creature-mouth" d="M79 116q5 9 11 0 6 9 11 0"/>'}<path class="creature-glint" d="M52 83q1-17 21-19"/>
    <rect class="creature-headphone" x="32" y="88" width="14" height="28" rx="6"/><path class="creature-antenna" d="M38 88V68"/><circle class="creature-led" cx="38" cy="65" r="4"/>
    </g>
    <g class="creature-console"><path class="console-side" d="m122 145 70-14 7 41-70 14Z"/><path class="console-screen" d="m122 110 68-14 4 43-68 14Z"/><path class="console-rim" d="m127 115 57-12 3 31-58 12Z"/>
    <g class="console-code"><path d="m136 121 6 3-5 5m12-8 18-4m-29 19 30-6"/></g><path class="console-keyboard" d="m126 153 61-12 7 22-62 13Z"/><path class="console-keys" d="m137 156 39-8m-35 14 39-8"/></g>
    <g class="creature-paw paw-left"><ellipse cx="119" cy="160" rx="12" ry="8" transform="rotate(-15 119 160)"/><path d="m116 158 1 4m4-5 1 4"/></g><g class="creature-paw paw-right"><ellipse cx="145" cy="151" rx="11" ry="8" transform="rotate(-15 145 151)"/><path d="m142 149 1 4m4-5 1 4"/></g>
    ${sleeping?'<g class="creature-snooze"><text x="158" y="64">z</text><text x="180" y="43">z</text></g>':''}
    ${issue?'<g class="creature-alert"><rect x="156" y="43" width="27" height="30" rx="11"/><path d="M170 51v9m0 6v1"/></g>':''}
    ${done?'<g class="creature-done"><circle cx="167" cy="55" r="14"/><path d="m160 55 5 5 9-11"/></g>':''}
    ${state==='pending'||state==='queued'?'<g class="creature-wait"><circle cx="166" cy="55" r="14"/><path d="M166 46v10l6 3"/></g>':''}
  </svg>`;
}

function studioBackdrop(){
  const buildings=Array.from({length:18},(_,i)=>{
    const x=36+i*70,h=45+(i*47%125),y=300-h;
    return `<g><path class="city-building" d="M${x} 305V${y}h52v${305-y}"/><path class="city-roof" d="M${x+5} ${y}v-9h24v9"/>${Array.from({length:4},(_,r)=>`<path class="city-window window-${(i+r)%3}" d="M${x+10} ${y+15+r*21}h8m10 0h8"/>`).join('')}</g>`;
  }).join('');
  const stars=Array.from({length:24},(_,i)=>`<circle cx="${65+i*49%1140}" cy="${43+i*37%153}" r="${i%3===0?1.8:1}" fill="${i%2?'#97c8ef':'#d6ceff'}" opacity=".6"/>`).join('');
  return `<defs><linearGradient id="station-sky" x2="0" y2="1"><stop stop-color="#10172e"/><stop offset="1" stop-color="#332650"/></linearGradient><linearGradient id="station-floor" x2="0" y2="1"><stop stop-color="#232442"/><stop offset="1" stop-color="#0c1728"/></linearGradient><radialGradient id="station-light"><stop stop-color="#67e8e0" stop-opacity=".13"/><stop offset="1" stop-color="#67e8e0" stop-opacity="0"/></radialGradient></defs>
    <g aria-hidden="true" class="station-scenery"><rect width="1280" height="870" fill="#101a2c"/><path d="M29 48h1222v277H29Z" fill="url(#station-sky)"/>${stars}
    <circle cx="994" cy="123" r="48" fill="#b3a5db"/><path d="M1017 80a48 48 0 0 0-3 88 48 48 0 0 0 3-88" fill="#262747"/><ellipse cx="994" cy="123" rx="77" ry="10" transform="rotate(-24 994 123)" fill="none" stroke="#7aaebd" stroke-width="2" opacity=".6"/>
    ${buildings}<path d="M25 315h1230V870H25Z" fill="url(#station-floor)"/><path class="station-horizon" d="M25 321h1230"/>
    ${Array.from({length:12},(_,i)=>`<path class="station-floor-line" d="M${450+i*34} 321 ${-750+i*250} 870"/>`).join('')}${[370,435,525,650,825].map(y=>`<path class="station-floor-line" d="M25 ${y}h1230"/>`).join('')}
    <ellipse cx="640" cy="540" rx="550" ry="280" fill="url(#station-light)"/>
    <path class="station-frame" d="M17 31h1246v305h-28V58H45v278H17ZM17 336h1246M340 58v257m600-257v257"/><path class="station-neon" d="M64 43h254m643 0h254"/>
    <path class="station-beam" d="m0 0 98 30h1084l98-30M0 0v870m1280-870v870"/>
    <path class="station-ceiling-light" d="M505 27h270"/><path class="station-ceiling-bracket" d="M485 0v35h310V0"/>
    <g class="station-wall-machine"><path d="M33 372h47v111H33Z"/><path d="M41 386h29v28H41Z"/><path d="M43 432h25m-25 10h25m-25 10h17"/><circle cx="57" cy="469" r="5"/></g>
    <g class="station-wall-machine"><path d="M1203 365h44v126h-44Z"/><path d="M1212 378h26v39h-26Z"/><path d="m1218 392 7-7 7 7m-14 8h14"/><circle cx="1225" cy="449" r="7"/><path d="M1213 471h24"/></g>
    <path class="station-foreground" d="m0 810 85 28h1110l85-28v60H0Z"/><path class="station-neon" d="M92 845h247m602 0h247"/>
    <g class="station-plant"><path d="M1155 752q-29-32-20-46 20 0 21 28 4-46 23-47 11 17-15 51 31-18 37-1-8 19-37 15"/><path d="M1141 750h40l-6 31h-28Z"/></g>
    <g class="station-crate"><path d="m87 751 42-10 31 17-42 12ZM87 751v31l31 20 42-13v-31m-42 12v32"/><path d="m100 761 17 8m15 3 17-5"/></g>
    </g>`;
}

function studioCore(snapshot){
  const tasks=(snapshot.board?.tasks||[]).filter(t=>['pending','running','waiting'].includes(t.state));
  return `<g class="station-core" aria-label="共享任务板 ${tasks.length} 项待完成"><ellipse class="core-shadow" cx="640" cy="575" rx="100" ry="30"/><path class="core-platform" d="M548 550v18c0 45 184 45 184 0v-18"/><ellipse class="core-platform-top" cx="640" cy="550" rx="92" ry="30"/>
    <path class="core-aerial" d="M604 546v-44m72 44v-44"/><rect class="core-case" x="550" y="399" width="180" height="115" rx="12"/><rect class="core-display" x="561" y="410" width="158" height="91" rx="6"/>
    ${tasks.slice(0,6).map((task,i)=>`<g><rect class="board-note ${task.state==='running'?'note-active':''}" x="${571+i%3*47}" y="${422+Math.floor(i/3)*35}" width="36" height="25" rx="3"/><path class="core-slot" d="M${577+i%3*47} ${430+Math.floor(i/3)*35}h20m-20 7h12"/><title>${esc(task.title)} ${esc(studioStates[task.state]||task.state)}</title></g>`).join('')}
    <text class="station-core-label" x="640" y="623" text-anchor="middle">共享任务板</text></g>`;
}

function studioPage(){
  const snapshot=studio.paused?.data||data,active=snapshot.instances.filter(a=>a.state!=='terminated');
  const busy=snapshot.runs.filter(r=>r.state==='running').length,queued=snapshot.runs.filter(r=>r.state==='queued').length;
  const selected=studio.view==='network'?snapshot.instances.find(a=>a.id===studio.selected):null;
  const goal=studioBoardGoal(snapshot),task=studio.view==='tasks'?goal?.tasks.find(t=>t.key===studio.selectedTask):null;
  return `<div class="studio ${studio.paused?'studio-paused':''} ${document.hidden?'studio-hidden':''}">
    <header class="studio-hud"><div class="studio-station-name"><span class="studio-station-mark" aria-hidden="true">◉</span><h1>夜航工作站</h1></div><div class="studio-tabs" role="group" aria-label="现场视图">${[['network','工作舱'],['tasks','接力'],['architecture','系统']].map(([key,title])=>`<button id="studio-view-${key}" data-action="studio-view" data-id="${key}" aria-pressed="${studio.view===key}" class="${studio.view===key?'active':''}">${title}</button>`).join('')}</div><div class="studio-live"><span id="studio-sync" role="status"></span><button class="studio-icon-button" data-action="studio-pause" title="${studio.paused?'继续实时同步':'暂停画面'}" aria-label="${studio.paused?'继续实时同步':'暂停画面'}">${studio.paused?'▶':'Ⅱ'}</button><button class="studio-icon-button" data-action="studio-fullscreen" title="全屏 / 退出全屏" aria-label="全屏 / 退出全屏">⛶</button></div></header>
    <div class="studio-scene-wrap">${studio.view==='architecture'?studioArchitecture(snapshot):studio.view==='tasks'?studioTasks(snapshot):studioNetwork(snapshot)}${selected||task?`<aside class="studio-inspect" aria-label="${task?'任务':'Agent'}详情"><button class="studio-inspect-close" data-action="studio-dismiss" aria-label="收起详情">×</button>${task?studioTaskDetail(goal,task,snapshot):studioAgentDetail(snapshot,selected.id)}</aside>`:''}</div>
    <footer class="studio-bottom-hud"><div class="studio-status-counts"><span><i class="count-busy"></i>${busy} 工作</span><span><i class="count-ready"></i>${active.filter(a=>a.state==='ready').length} 待命</span><span><i class="count-queued"></i>${queued} 排队</span></div><span class="studio-bottom-note">${studio.view==='architecture'?'结构示意':studio.view==='tasks'?'连线 任务拆分':'点选队员'}</span><a href="#board" class="studio-log-link" title="发布任务">${icon('plus')}<span class="sr-only">发布任务</span></a></footer>
  </div>`;
}

function studioEventName(kind){
  const words={'run.queued':'排队','run.started':'开始工作','run.settled':'已结算','execution.model_started':'请求模型','execution.model_completed':'模型返回','execution.tool_started':'执行工具','execution.tool_completed':'工具返回','execution.started':'开始执行','execution.finished':'执行结束'};
  return words[kind]||kind;
}
function studioRunPhase(run){
  if(!run)return '';
  const trace=(studio.paused?.events||events).findLast(e=>e.subject===run.id&&e.kind.startsWith('execution.'));
  return trace?studioEventName(trace.kind):'工作中';
}

function studioNetwork(snapshot){
  const source=snapshot.instances.filter(a=>studio.scope==='all'||a.state!=='terminated');
  const pages=Math.max(1,Math.ceil(source.length/9));studio.page=Math.max(0,Math.min(studio.page,pages-1));
  const agents=source.slice(studio.page*9,studio.page*9+9),positions=new Map(agents.map((a,i)=>[a.id,studioSpots[i]]));
  const groups=new Map();
  for(const m of snapshot.messages||[]){
    if(!positions.has(m.sender_instance_id)||!positions.has(m.recipient_id))continue;
    const key=JSON.stringify([m.sender_instance_id,m.recipient_id]);
    if(!groups.has(key))groups.set(key,[]);groups.get(key).push(m);
  }
  const selected=agents.some(a=>a.id===studio.selected)?studio.selected:null;
  const wires=(snapshot.board?.tasks||[]).filter(t=>t.state==='running'&&t.instance_id&&positions.has(t.instance_id)).map(t=>{const p=positions.get(t.instance_id);return `<path class="station-conduit conduit-active" d="M640 510Q640 ${p.y+165} ${p.x+95} ${p.y+165}"><title>已认领任务：${esc(t.title)} ${esc(t.instance_id)}</title></path>`;}).join('');
  const messages=[...groups.values()].map(list=>{
    const m=list[0],a=positions.get(m.sender_instance_id),b=positions.get(m.recipient_id),flowing=list.some(m=>['queued','running'].includes(m.inbox_state));
    const x=a.x+95,y=a.y+170,tx=b.x+95,ty=b.y+170;
    const path=m.sender_instance_id===m.recipient_id?`M${x+60} ${y}c100-130 100 70 0 0`:`M${x} ${y}Q${(x+tx)/2} ${(y+ty)/2-80} ${tx} ${ty}`;
    return `<g class="station-message ${flowing?'is-flowing':''} ${selected&&![m.sender_instance_id,m.recipient_id].includes(selected)?'is-muted':''}"><path class="studio-edge" d="${path}" marker-end="url(#station-arrow)"><title>${esc(m.sender_instance_id)} → ${esc(m.recipient_id)} ${list.length} 条消息</title></path>${flowing?[.25,.5,.75].map((t,i)=>`<circle class="message-packet packet-${i}" cx="${x+(tx-x)*t}" cy="${(1-t)*(1-t)*y+2*(1-t)*t*((y+ty)/2-80)+t*t*ty}" r="4"/>`).join(''):''}</g>`;
  }).join('');
  return `<section class="studio-stage" aria-label="Agent 工作舱"><div class="studio-scene-controls"><button class="studio-small-button" data-action="studio-scope" aria-pressed="${studio.scope==='all'}">${studio.scope==='all'?'全部队员':'在场队员'}</button><div class="studio-pagination"><button class="studio-icon-button" data-action="studio-prev" aria-label="上一组" ${studio.page===0?'disabled':''}>‹</button><span>${studio.page+1} / ${pages}</span><button class="studio-icon-button" data-action="studio-next" aria-label="下一组" ${studio.page===pages-1?'disabled':''}>›</button></div></div>
    <div class="studio-map-scroll"><svg class="studio-map" viewBox="0 0 1280 870" role="group" aria-label="赛博工作站，点选队员查看任务"><defs><marker id="station-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="m1 1 8 4-8 4" fill="none" stroke="#ffb8e5" stroke-width="1.5"/></marker></defs>${studioBackdrop()}${wires}${messages}${studioCore(snapshot)}${agents.map(a=>{
      const p=positions.get(a.id),run=snapshot.runs.find(r=>r.instance_id===a.id&&r.state==='running'),visualState=a.state==='busy'&&!run?'queued':a.state,phase=run?studioRunPhase(run):studioStates[visualState]||labels[visualState]||visualState;
      const summary=`${a.preset_name} ${phase}${a.snapshot?.protocol==='demo'?' Demo':''}${run?' '+run.input:''}`;
      return `<foreignObject x="${p.x}" y="${p.y}" width="190" height="235"><div xmlns="http://www.w3.org/1999/xhtml" class="studio-desk-wrap"><button id="studio-agent-${esc(a.id)}" class="studio-desk state-${esc(visualState)} ${selected===a.id?'selected':''}" data-action="studio-select" data-id="${esc(a.id)}" aria-label="${esc(summary)}" title="${esc(summary)}" aria-pressed="${selected===a.id}">${studioPerson(visualState,a.id,phase)}<span class="studio-crew-name">${esc(a.preset_name)}<small>${a.snapshot?.protocol==='demo'?'DEMO ':''}${esc(a.id.slice(-4))}</small></span></button></div></foreignObject>`;
    }).join('')}</svg></div>${!agents.length?`<div class="studio-empty-room"><span>等待队员入舱</span><a href="#board">发布任务</a></div>`:''}</section>`;
}

function studioAgentDetail(snapshot,id){
  const a=snapshot.instances.find(a=>a.id===id);
  if(!a)return '';
  const runs=snapshot.runs.filter(r=>r.instance_id===id),current=runs.find(r=>r.state==='running')||runs.find(r=>r.state==='queued')||runs[0];
  const messages=(snapshot.messages||[]).filter(m=>m.sender_instance_id===id||m.recipient_id===id);
  return `${studioPerson(a.state,a.id,current?studioRunPhase(current):'')}<h2>${esc(a.preset_name)}</h2><div class="studio-detail-meta">${esc(studioStates[a.state]||a.state)} ${esc(a.snapshot?.protocol||'未知协议')} #${esc(a.id.slice(-5))}</div>${current?`<p class="studio-full-text">${esc(current.input)}</p><div class="studio-detail-meta">${esc(studioStates[current.state]||labels[current.state]||current.state)}</div>`:'<p>暂无执行记录</p>'}<div class="studio-inspect-actions">${current?button('运行详情','run-detail',false,`data-id="${esc(current.id)}"`):''}${button('实例','agent-detail',false,`data-id="${esc(id)}"`)}</div>${messages.length?`<details id="studio-messages"><summary>消息 ${messages.length}</summary>${messages.slice(0,8).map(m=>`<button class="studio-message" data-action="message-detail" data-id="${esc(m.id)}"><span>${m.sender_instance_id===id?'→':'←'} ${esc(snapshot.instances.find(x=>x.id===(m.sender_instance_id===id?m.recipient_id:m.sender_instance_id))?.preset_name||(m.sender_instance_id?'其他队员':'操作者'))}</span><small>${esc(messageLabels[m.inbox_state]||messageLabels[m.delivery_state]||m.delivery_state)}</small></button>`).join('')}</details>`:''}`;
}

function studioTaskLayout(tasks){
  const byKey=new Map(tasks.map(t=>[t.key,t])),depth=new Map();
  function visit(key,seen=new Set()){
    if(depth.has(key))return depth.get(key);
    if(seen.has(key)||!byKey.has(key))return 0;
    const next=new Set(seen);next.add(key);
    const rank=Math.max(0,...(byKey.get(key).depends_on||[]).filter(k=>byKey.has(k)).map(k=>visit(k,next)+1));
    depth.set(key,rank);return rank;
  }
  tasks.forEach(t=>visit(t.key));
  const rows=new Map(),positions=new Map();
  for(const t of tasks){const rank=depth.get(t.key),row=rows.get(rank)||0;rows.set(rank,row+1);positions.set(t.key,{x:25+rank*285,y:35+row*235});}
  return {positions,width:Math.max(860,(Math.max(0,...depth.values())+1)*285),height:Math.max(520,Math.max(0,...rows.values())*235+45)};
}

function studioBoardGoal(snapshot){
  const tasks=snapshot.board?.tasks||[],roots=tasks.filter(t=>!t.parent_id);
  const root=roots.find(t=>t.id===studio.boardRootId)||roots.find(t=>['pending','running','waiting'].includes(t.state))||roots[0];
  if(!root)return null;
  return {...root,tasks:tasks.filter(t=>t.root_id===root.id||t.id===root.id).map(t=>({...t,key:t.id,display_title:t.title,depends_on:t.parent_id?[t.parent_id]:[]}))};
}

function studioTasks(snapshot){
  const g=studioBoardGoal(snapshot),roots=(snapshot.board?.tasks||[]).filter(t=>!t.parent_id);
  const selector=`<select id="studio-board-root" aria-label="选择任务" ${studio.paused?'disabled':''}>${roots.length?roots.map(root=>`<option value="${esc(root.id)}" ${root.id===g?.id?'selected':''}>${esc(root.title)}</option>`).join(''):'<option value="">暂无任务</option>'}</select>`;
  let body='';
  if(!g)body='<div class="studio-scene-empty"><span>暂无任务</span><a href="#board">发布任务</a></div>';
  else{
    const tasks=g.tasks||[],layout=studioTaskLayout(tasks),point=k=>layout.positions.get(k);
    const path=(a,b)=>`M${a.x+170} ${a.y+155}C${a.x+260} ${a.y+155} ${b.x-25} ${b.y+155} ${b.x+60} ${b.y+155}`;
    body=`<div class="studio-task-scroll" tabindex="0" role="region" aria-label="任务拆分图，可横向滚动"><svg width="${layout.width}" height="${layout.height}" role="group" aria-label="任务按拆分来源从左向右排列"><defs><marker id="studio-task-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="m1 1 8 4-8 4" fill="none" stroke="#72e4df" stroke-width="1.5"/></marker></defs>${tasks.map(t=>(t.depends_on||[]).filter(k=>point(k)).map(k=>`<path class="studio-edge task-dependency" d="${path(point(k),point(t.key))}" marker-end="url(#studio-task-arrow)"><title>${esc(k)} → ${esc(t.key)}：拆分来源</title></path>`).join('')).join('')}${tasks.map(t=>{
      const p=point(t.key),a=snapshot.instances.find(a=>a.id===t.instance_id),state=studioStates[t.state]||labels[t.state]||t.state;
      return `<foreignObject x="${p.x}" y="${p.y}" width="240" height="220"><div xmlns="http://www.w3.org/1999/xhtml" class="studio-task-wrap"><button class="studio-task state-${esc(t.state)} ${studio.selectedTask===t.key?'selected':''}" data-action="studio-task-select" data-id="${esc(t.key)}" aria-label="${esc(t.display_title||t.key)} ${esc(state)} ${esc(a?.preset_name||'未分配')}" title="${esc(t.display_title||t.key)} ${esc(state)}" aria-pressed="${studio.selectedTask===t.key}">${studioPerson(t.state,t.instance_id||t.key)}<span class="studio-crew-name">${esc(t.display_title||t.key)}<small>${esc(state)}</small></span></button></div></foreignObject>`;
    }).join('')}</svg></div>`;
  }
  return `<section class="studio-stage studio-task-stage" aria-label="任务拆分"><svg class="studio-stage-background" viewBox="0 0 1280 870" preserveAspectRatio="xMidYMid slice" aria-hidden="true">${studioBackdrop()}</svg><div class="studio-scene-controls">${selector}${g?`<a class="studio-small-button" href="#board/${esc(g.id)}" title="查看任务与验收标准">${(g.tasks||[]).filter(t=>t.state==='completed').length} / ${(g.tasks||[]).length} 完成</a>`:''}</div>${body}</section>`;
}

function studioTaskDetail(goal,t,snapshot){
  const a=snapshot.instances.find(a=>a.id===t.instance_id),waiting=t.state==='pending'?(t.preset_name?'等待认领':'等待选型'):t.state==='waiting'?'等待子任务':studioStates[t.state]||labels[t.state]||t.state;
  return `${studioPerson(t.state,t.instance_id||t.key)}<h2>${esc(t.display_title||t.key)}</h2><div class="studio-detail-meta">${esc(waiting)} ${esc(a?.preset_name||t.preset_name||'待选型')}</div><p class="studio-full-text">${esc(t.instructions)}</p><div class="studio-inspect-actions">${t.run_id?button('运行详情','run-detail',false,`data-id="${esc(t.run_id)}"`):''}<a class="button" href="#board/${esc(t.id)}">任务详情</a></div>`;
}

function studioModule(kind){
  const base='<ellipse class="module-pad" cx="110" cy="175" rx="90" ry="25"/><path class="module-pad-rim" d="M23 181q87 39 174 0"/>';
  const drawings={
    target:'<path class="module-hologram" d="M62 55h96v92H62Z"/><circle class="module-line" cx="110" cy="98" r="30"/><circle class="module-line" cx="110" cy="98" r="17"/><path class="module-pink" d="m99 99 9 9 28-31"/><path class="module-light" d="M82 165h56M110 147v18"/>',
    scheduler:'<path class="module-solid" d="M63 62v87c0 25 94 25 94 0V62"/><ellipse class="module-top" cx="110" cy="62" rx="47" ry="17"/><path class="module-light" d="M79 88q31 13 62 0m-62 24q31 13 62 0m-62 24q31 13 62 0"/><path class="module-line" d="M110 44V25"/><circle class="module-pink-fill" cx="110" cy="22" r="6"/>',
    agents:'<g transform="translate(-5 5) scale(.85)">'+studioPerson('ready','crew')+'</g>',
    messages:'<path class="module-solid" d="M52 76h116v78H52Z"/><path class="module-light" d="m54 79 56 40 56-40"/><path class="module-pink" d="M76 47h39m-28-12h32m26 26h22"/><path class="module-light" d="M89 168h42"/>',
    storage:'<path class="module-solid" d="M54 57h45v102H54Zm67 0h45v102h-45Z"/><path class="module-light" d="M65 74h23m-23 17h23m44-17h23m-23 17h23M65 113h23m44 0h23"/><circle class="module-pink-fill" cx="77" cy="144" r="5"/><circle class="module-pink-fill" cx="143" cy="144" r="5"/>',
    tools:'<path class="module-solid" d="M66 154h90v18H66Zm35-23h20v24h-20Z"/><path class="module-arm" d="m110 135-35-42 40-42 34 27"/><circle class="module-joint" cx="75" cy="93" r="12"/><circle class="module-joint" cx="115" cy="51" r="12"/><path class="module-light" d="m148 77 17 6m-17-6 7 20m-18-13 10 14"/>',
  };
  return `<svg class="studio-module-art" viewBox="0 0 220 210" aria-hidden="true" fill="none" stroke-linecap="round" stroke-linejoin="round">${base}${drawings[kind]}</svg>`;
}

function studioArchitecture(){
  const modules=[['target','任务板','board',130,270],['scheduler','控制面 选型 / 生命周期','presets',530,190],['agents','对等 Agent','agents',930,270],['messages','消息','messages',130,565],['storage','存储','artifacts',530,580],['tools','Harness 模型 / 工具','plugins',930,565]];
  return `<section class="studio-stage studio-architecture" aria-label="系统结构示意"><span class="studio-structure-label">系统剖面</span><svg class="studio-map" viewBox="0 0 1280 870" role="group" aria-label="任务板、控制面选型、对等 Agent 与标准 Harness 的结构关系">${studioBackdrop()}<g class="module-rails"><path d="M240 445 640 365 1040 445M240 445v295l400 15 400-15V445M640 365v390M240 740l800-295"/></g>${modules.map(([kind,title,href,x,y])=>`<foreignObject x="${x}" y="${y}" width="220" height="235"><div xmlns="http://www.w3.org/1999/xhtml" class="studio-machine-wrap"><a class="studio-machine" href="#${href}" title="查看${esc(title)}">${studioModule(kind)}<span>${title}</span></a></div></foreignObject>`).join('')}</svg></section>`;
}

async function studioAction(action,element){
  if(!action.startsWith('studio-'))return false;
  if(action==='studio-view'){studio.view=element.dataset.id;studio.selected=null;studio.selectedTask=null;render();}
  if(action==='studio-select'){studio.selected=studio.selected===element.dataset.id?null:element.dataset.id;render();}
  if(action==='studio-task-select'){studio.selectedTask=studio.selectedTask===element.dataset.id?null:element.dataset.id;render();}
  if(action==='studio-dismiss'){
    const actor=studio.selected,task=studio.selectedTask;
    studio.selected=null;studio.selectedTask=null;render();
    const trigger=actor?document.getElementById(`studio-agent-${actor}`):[...document.querySelectorAll('[data-action="studio-task-select"]')].find(el=>el.dataset.id===task);
    trigger?.focus({preventScroll:true});
  }
  if(action==='studio-scope'){studio.scope=studio.scope==='live'?'all':'live';studio.page=0;studio.selected=null;render();}
  if(action==='studio-prev'||action==='studio-next'){studio.page+=action==='studio-next'?1:-1;studio.selected=null;render();}
  if(action==='studio-pause'){
    if(studio.paused){studio.paused=null;render();await refresh(true);}
    else{studio.paused={data:structuredClone(data),events:structuredClone(events),goal:structuredClone(studio.goal),goalError:studio.goalError,at:studio.syncedAt};render();}
  }
  if(action==='studio-retry'){studio.goalError='';await studioReadGoal();render();}
  if(action==='studio-fullscreen'){
    try{if(document.fullscreenElement)await document.exitFullscreen();else await $('#content').requestFullscreen();}
    catch{toast('当前窗口不支持全屏。');}
  }
  return true;
}
document.addEventListener('change',async event=>{
  if(event.target.id==='studio-board-root'){
    if(studio.paused)return;
    studio.boardRootId=event.target.value;studio.selectedTask=null;render();return;
  }
  if(event.target.id!=='studio-goal')return;
  if(studio.paused)return;
  studio.goalId=event.target.value;studio.goal=null;studio.goalError='';studio.selectedTask=null;render();
  await studioReadGoal();render();
});
document.addEventListener('visibilitychange',()=>{document.querySelector('.studio')?.classList.toggle('studio-hidden',document.hidden);});
document.addEventListener('keydown',event=>{
  if(event.key==='Escape'&&!document.fullscreenElement&&!$('#modal').open&&page()==='studio'&&(studio.selected||studio.selectedTask))studioAction('studio-dismiss',event.target);
});
