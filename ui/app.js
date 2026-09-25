const invoke = window.__TAURI__.core.invoke;
const $ = s => document.querySelector(s);
const messages = $('#messages');
const conversation = $('#conversation');
const composer = $('#composer');
const sendBtn = $('#sendBtn');
const thinking = $('#thinking');
const thinkingText = $('#thinkingText');
const strip = $('#attachmentStrip');
const welcome = $('#welcome');
const statusDot = $('#statusDot');
const statusText = $('#statusText');
const modelPicker = $('#modelPicker');
const modelButton = $('#modelButton');
const modelButtonText = $('#modelButtonText');
const modelMenu = $('#modelMenu');
const dynamicModels = $('#dynamicModels');
const greeting = $('#greeting');
const pinBtn = $('#pinBtn');
const computerControlBtn = $('#computerControlBtn');
const activitySheet = $('#activitySheet');
const opsSheet = $('#opsSheet');
const inlineStop = $('#inlineStop');
const shareSheet = $('#shareSheet');
const closePanelBtn = $('#closePanelBtn');
const panelSide = $('#panelSide');
const panelWidth = $('#panelWidth');
const panelWidthValue = $('#panelWidthValue');

let attachments = [];
let busy = false;
let stopping = false;
let sessionId = localStorage.getItem('fatir-session') || localStorage.getItem('ah-session') || crypto.randomUUID();
localStorage.setItem('fatir-session', sessionId);
let selectedModel = localStorage.getItem('fatir-model') || localStorage.getItem('ah-model') || 'auto';
let currentProactiveEvent = null;
let proactiveConfigCache = null;
let sharePaths = [];
let shareFiles = [];
let whatsappAllContacts = [];
let shareAutoPinned = false;
let panelLayoutTimer = null;
if (selectedModel === 'vision' || selectedModel === 'qwen-vision') selectedModel = 'gemma4:31b-cloud';

const baseModels = new Map([
  ['auto','Auto'],
  ['local','Local only'],
  ['cloud','Cloud only'],
  ['gpt-oss:20b-cloud','GPT-OSS 20B'],
  ['gpt-oss:120b-cloud','GPT-OSS 120B'],
  ['gemma4:31b-cloud','Gemma 4 31B'],
  ['nemotron-3-nano:30b-cloud','Nemotron 3 Nano'],
  ['nemotron-3-super:cloud','Nemotron 3 Super'],
  ['nemotron-3-ultra:cloud','Nemotron 3 Ultra']
]);


function formatBytes(bytes){
  const n=Number(bytes)||0;
  if(n<1024) return `${n} B`;
  const units=['KB','MB','GB','TB']; let v=n/1024, i=0;
  while(v>=1024 && i<units.length-1){v/=1024;i++;}
  return `${v>=100?v.toFixed(0):v>=10?v.toFixed(1):v.toFixed(2)} ${units[i]}`;
}
function percent(part,total){ return total>0 ? Math.round((part/total)*100) : 0; }
function setGreeting(){
  const h=new Date().getHours();
  greeting.textContent = h<12 ? 'Good morning' : h<17 ? 'Good afternoon' : 'Good evening';
}
function shortModelLabel(value,label){
  if(value==='auto') return 'Auto';
  if(value==='local') return 'Local only';
  if(value==='cloud') return 'Cloud only';
  return (label||baseModels.get(value)||value||'Model').replace(/\s+Cloud.*$/i,'').slice(0,24);
}
function formatWhen(value){
  try{
    const d=new Date(value); if(Number.isNaN(d.getTime())) return '';
    const diff=Date.now()-d.getTime();
    if(diff<60000) return 'now';
    if(diff<3600000) return `${Math.max(1,Math.round(diff/60000))}m`;
    if(diff<86400000) return `${Math.round(diff/3600000)}h`;
    return d.toLocaleDateString(undefined,{month:'short',day:'numeric'});
  }catch{return ''}
}

function escapeHtml(s='') { return String(s).replace(/[&<>'"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;',"'":'&#039;','"':'&quot;'}[c])); }
function inlineMd(raw=''){
  let out=''; let last=0;
  const re=/`([^`]+)`/g; let m;
  while((m=re.exec(raw))){
    out+=inlineMdPlain(raw.slice(last,m.index));
    const code=m[1];
    if(code.startsWith('/')||code.startsWith('~/')){
      out+=`<button type="button" class="inline-path" data-path="${escapeHtml(code)}" title="Open ${escapeHtml(code)}"><code>${escapeHtml(code)}</code></button>`;
    }else out+=`<code>${escapeHtml(code)}</code>`;
    last=m.index+m[0].length;
  }
  out+=inlineMdPlain(raw.slice(last));
  return out;
}
function inlineMdPlain(raw=''){
  let x=escapeHtml(raw);
  x=x.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,'<a class="md-link" href="$2" data-url="$2">$1</a>');
  x=x.replace(/\*\*([^*]+)\*\*/g,'<strong>$1</strong>');
  x=x.replace(/__([^_]+)__/g,'<strong>$1</strong>');
  x=x.replace(/(^|\s)\*([^*\n]+)\*(?=\s|$)/g,'$1<em>$2</em>');
  return x;
}
function renderMarkdown(source=''){
  const text=String(source).replace(/\r\n?/g,'\n');
  const blocks=[];
  const tokenized=text.replace(/```([^\n`]*)\n?([\s\S]*?)```/g,(_,lang,code)=>{
    const token=`@@FATIR_CODE_${blocks.length}@@`;
    blocks.push(`<div class="md-scroll"><pre><code class="language-${escapeHtml((lang||'').trim())}">${escapeHtml(code.replace(/^\n|\n$/g,''))}</code></pre></div>`);
    return token;
  });
  const lines=tokenized.split('\n');
  const html=[];
  let list=null;
  const closeList=()=>{if(list){html.push(`</${list}>`);list=null;}};
  for(let i=0;i<lines.length;i++){
    const line=lines[i]; const trimmed=line.trim();
    if(/^@@FATIR_CODE_\d+@@$/.test(trimmed)){closeList();html.push(trimmed);continue;}
    if(!trimmed){closeList();continue;}
    if(trimmed.includes('|') && i+1<lines.length && /^\s*\|?\s*:?-{3,}/.test(lines[i+1])){
      closeList(); const headers=trimmed.replace(/^\||\|$/g,'').split('|').map(x=>x.trim()); i+=1;
      const rows=[];
      while(i+1<lines.length && lines[i+1].includes('|') && lines[i+1].trim()){
        i+=1; rows.push(lines[i].trim().replace(/^\||\|$/g,'').split('|').map(x=>x.trim()));
      }
      html.push(`<div class="md-scroll"><table><thead><tr>${headers.map(h=>`<th>${inlineMd(h)}</th>`).join('')}</tr></thead><tbody>${rows.map(r=>`<tr>${r.map(c=>`<td>${inlineMd(c)}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`);
      continue;
    }
    const h=trimmed.match(/^(#{1,4})\s+(.+)$/); if(h){closeList();const n=h[1].length;html.push(`<h${n}>${inlineMd(h[2])}</h${n}>`);continue;}
    const ul=trimmed.match(/^[-*+]\s+(.+)$/); if(ul){if(list!=='ul'){closeList();list='ul';html.push('<ul>')}html.push(`<li>${inlineMd(ul[1])}</li>`);continue;}
    const ol=trimmed.match(/^\d+[.)]\s+(.+)$/); if(ol){if(list!=='ol'){closeList();list='ol';html.push('<ol>')}html.push(`<li>${inlineMd(ol[1])}</li>`);continue;}
    const q=trimmed.match(/^>\s?(.*)$/); if(q){closeList();html.push(`<blockquote>${inlineMd(q[1])}</blockquote>`);continue;}
    closeList(); html.push(`<p>${inlineMd(trimmed)}</p>`);
  }
  closeList();
  let result=html.join('');
  blocks.forEach((block,i)=>{result=result.replace(`@@FATIR_CODE_${i}@@`,block)});
  return result;
}
function wireRichContent(holder){
  holder.querySelectorAll('.inline-path').forEach(btn=>btn.addEventListener('click',()=>invoke('open_local_path',{path:btn.dataset.path}).catch(addError)));
  holder.querySelectorAll('.md-link').forEach(a=>a.addEventListener('click',e=>{e.preventDefault(); const url=a.dataset.url; if(url) invoke('open_external_url',{url}).catch(addError)}));
}
function formatText(s='') { return renderMarkdown(s); }
function scrollBottom(){ requestAnimationFrame(()=>conversation.scrollTo({top:conversation.scrollHeight,behavior:'smooth'})); }
function modelLabel(m){ const x=String(m||''); if(x.includes('fast path')) return 'Fast Path · 0 LLM'; if(x.includes('fatir-router')) return 'Local · Router'; if(x.includes('fatir-local')) return 'Local · Deep'; if(x.includes('qwen3.5')&&!x.includes(':cloud')) return 'Local · Qwen3.5'; if(x.includes('gemma4')) return 'Cloud · Gemma 4'; if(x.includes('nemotron-3-nano')) return 'Cloud · Nemotron Nano'; if(x.includes('nemotron-3-super')) return 'Cloud · Nemotron Super'; if(x.includes('nemotron-3-ultra')) return 'Cloud · Nemotron Ultra'; if(x.includes('gpt-oss:20b')) return 'Cloud · GPT-OSS 20B'; if(x.includes('gpt-oss:120b')) return 'Cloud · GPT-OSS 120B'; return x || 'Auto'; }
function setSelectedModel(value,label){
  selectedModel = value;
  localStorage.setItem('fatir-model',value);
  modelButtonText.textContent = shortModelLabel(value,label || baseModels.get(value) || value);
  document.querySelectorAll('.model-option').forEach(b=>b.classList.toggle('selected',b.dataset.value===value));
  modelMenu.classList.add('hidden');
  modelButton.setAttribute('aria-expanded','false');
}
function currentModel(){ return selectedModel || 'auto'; }

function setBusy(value){
  busy = value;
  if(value){
    sendBtn.textContent='■';
    sendBtn.classList.add('stop-mode');
    sendBtn.disabled=false;
    sendBtn.setAttribute('aria-label','Stop');
    thinking.classList.remove('hidden');
    thinkingText.textContent = stopping ? 'Stopping…' : 'Fatir is working';
  } else {
    stopping=false;
    sendBtn.textContent='↑';
    sendBtn.classList.remove('stop-mode');
    sendBtn.disabled=false;
    sendBtn.setAttribute('aria-label','Send');
    thinking.classList.add('hidden');
    thinkingText.textContent='Fatir is working';
  }
}

function addUser(text){
  welcome.classList.add('hidden');
  const el=document.createElement('div'); el.className='message user';
  el.innerHTML=`<div class="bubble">${formatText(text)}</div>`;
  messages.appendChild(el); wireRichContent(el); scrollBottom();
}

function resourceHtml(resources=[]){
  if(!resources.length) return '';
  return `<div class="resource-list">${resources.map((r,i)=>{
    const icon=r.kind==='folder'?'▱':'◻';
    const share=r.kind==='folder'?'':`<button type="button" class="resource-share" data-resource-index="${i}" title="Share ${escapeHtml(r.label||r.path)}">↗</button>`;
    return `<div class="resource-row"><button type="button" class="resource-chip resource-open" data-resource-index="${i}" title="${escapeHtml(r.path)}"><span>${icon}</span><strong>${escapeHtml(r.label||r.path)}</strong></button>${share}</div>`;
  }).join('')}</div>`;
}

function wireResources(holder, resources=[]){
  holder.querySelectorAll('.resource-open').forEach(btn=>{
    const idx=Number(btn.dataset.resourceIndex); const resource=resources[idx]; if(!resource)return;
    btn.addEventListener('click',async()=>{try{await invoke('open_local_path',{path:resource.path});}catch(e){addError(e)}});
  });
  holder.querySelectorAll('.resource-share').forEach(btn=>{
    const idx=Number(btn.dataset.resourceIndex); const resource=resources[idx]; if(!resource)return;
    btn.addEventListener('click',e=>{e.stopPropagation();openShare([resource.path]);});
  });
}

function prettyTool(name=''){
  const map={
    amazon_keyword_research:'Amazon research',browser_click_text:'Browser action',browser_fill_by_label:'Fill webpage field',browser_page_summary:'Read webpage',
    browser_elements:'Inspect webpage controls',browser_click_element:'Activate webpage control',browser_upload_file:'Upload file to webpage',browser_open_url:'Open webpage',browser_get_url:'Read webpage URL',
    run_shell_command:'Terminal command',run_privileged_command:'Administrator command',background_job_start:'Background job',read_file:'Read file',render_pdf_pages:'Read PDF pages',
    cleanup_scan:'Cleanup scan',duplicate_scan:'Duplicate scan',trash_inventory:'Trash scan',desktop_find:'Find app control',desktop_activate_named:'Use app control',
    delegate_specialist:'Specialist check',system_snapshot:'System snapshot',health_report:'Health check'
  };
  return map[name]||String(name).replaceAll('_',' ');
}

function addAssistant(res){
  welcome.classList.add('hidden');
  const el=document.createElement('div'); el.className='message assistant';
  const allResources=[];
  let trace='';
  if(res.trace?.length){
    trace='<div class="trace">'+res.trace.map(t=>{
      const offset=allResources.length;
      const resources=t.resources||[];
      allResources.push(...resources);
      const resourceBlock=resources.length
        ? `<div class="resource-list">${resources.map((r,j)=>`<div class="resource-row"><button type="button" class="resource-chip resource-open" data-resource-index="${offset+j}" title="${escapeHtml(r.path)}"><span>${r.kind==='folder'?'▱':'◻'}</span><strong>${escapeHtml(r.label||r.path)}</strong></button>${r.kind==='folder'?'':`<button type="button" class="resource-share" data-resource-index="${offset+j}" title="Share ${escapeHtml(r.label||r.path)}">↗</button>`}</div>`).join('')}</div>`
        : '';
      const meta=t.status==='meta';
      const icon=t.status==='error'?'!':meta?'⌁':'✓';
      const detail=t.detail?`<details class="trace-detail" ${t.status==='error'?'open':''}><summary>${meta?'Efficiency':'Details'}</summary><div>${escapeHtml(t.detail)}</div></details>`:'';
      return `<div class="trace-item ${t.status==='error'?'error':''} ${meta?'meta':''}"><div class="trace-icon">${icon}</div><div class="trace-text"><strong>${escapeHtml(prettyTool(t.title))}</strong>${detail}${resourceBlock}</div></div>`;
    }).join('')+'</div>';
  }
  let approval='';
  if(res.pending){
    const p=res.pending;
    approval=`<div class="approval" data-id="${escapeHtml(p.id)}"><div class="approval-head"><div class="approval-icon">!</div><div><strong>${escapeHtml(p.summary)}</strong><p>${p.risk==='destructive'?'This changes or removes local data.':p.risk==='sensitive'?'This uses a stored secret without exposing its value to the model.':'This will make a change on your computer.'}</p></div></div><div class="approval-actions"><button class="deny">Cancel</button><button class="approve">Approve</button></div></div>`;
  }
  el.innerHTML=`<div class="assistant-label">Fatir <span class="model-tag">${escapeHtml(modelLabel(res.model||''))}</span></div><div class="bubble">${formatText(res.text||'')}</div>${trace}${approval}`;
  messages.appendChild(el);
  wireRichContent(el);
  wireResources(el,allResources);
  if(res.pending){
    el.querySelector('.approve').addEventListener('click',()=>resolveApproval(res.pending.id,true,el));
    el.querySelector('.deny').addEventListener('click',()=>resolveApproval(res.pending.id,false,el));
  }
  scrollBottom();
}

function addNotice(text){
  const el=document.createElement('div'); el.className='run-notice'; el.textContent=text; messages.appendChild(el); scrollBottom();
}
function addError(err){
  const text=String(err);
  if(text.includes('FATIR_STOPPED')){ addNotice('Stopped.'); return; }
  const el=document.createElement('div');el.className='message assistant';
  el.innerHTML=`<div class="assistant-label">Fatir</div><div class="bubble">I hit a problem: <strong>${escapeHtml(text)}</strong></div>`;messages.appendChild(el);scrollBottom();
}

async function stopFlow(){
  if(!busy || stopping) return;
  stopping=true;
  thinkingText.textContent='Stopping…';
  sendBtn.classList.add('stopping');
  try{ await invoke('cancel_run',{sessionId}); }
  catch(e){ console.warn('Could not cancel Fatir run',e); }
}

async function resolveApproval(id,approve,holder){
  if(busy) return;
  setBusy(true);
  holder.querySelector('.approval')?.classList.add('hidden');
  scrollBottom();
  try{
    const cmd=approve?'approve_action':'deny_action';
    const res=await invoke(cmd,{actionId:id,mode:currentModel()});
    addAssistant(res);
  }catch(e){ addError(e); }
  finally{ sendBtn.classList.remove('stopping'); setBusy(false); }
}

async function send(textOverride){
  const text=(textOverride??composer.value).trim();
  if(!text||busy) return;
  const outgoing=[...attachments];
  addUser(text);
  composer.value=''; autoSize(); clearAttachments();
  setBusy(true); scrollBottom();
  try{
    const res=await invoke('send_message',{sessionId,text,attachments:outgoing.map(path=>({path})),mode:currentModel()});
    addAssistant(res);
  }catch(e){ addError(e); }
  finally{
    sendBtn.classList.remove('stopping');
    setBusy(false);
    refreshStatus();
  }
}

async function addFiles(){ try{ const files=await invoke('pick_files'); attachments.push(...files.filter(x=>!attachments.includes(x))); renderAttachments(); }catch(e){ addError(e); } }
async function takeShot(){ try{ const path=await invoke('capture_screenshot'); attachments.push(path); renderAttachments(); }catch(e){ addError(e); } }
function renderAttachments(){
  strip.innerHTML=''; strip.classList.toggle('hidden',attachments.length===0);
  attachments.forEach((path,i)=>{
    const name=path.split('/').pop(); const ext=(name.split('.').pop()||'').toUpperCase().slice(0,4);
    const el=document.createElement('div'); el.className='attachment';
    el.innerHTML=`<button type="button" class="attachment-open" title="Open ${escapeHtml(path)}"><div class="file-icon">${escapeHtml(ext||'FILE')}</div><div class="file-name">${escapeHtml(name)}</div></button><button class="attachment-remove" type="button">×</button>`;
    el.querySelector('.attachment-open').onclick=()=>invoke('open_local_path',{path}).catch(addError);
    el.querySelector('.attachment-remove').onclick=()=>{attachments.splice(i,1);renderAttachments()};
    strip.appendChild(el);
  });
}
function clearAttachments(){ attachments=[]; renderAttachments(); }
function autoSize(){ composer.style.height='auto'; composer.style.height=Math.min(composer.scrollHeight,110)+'px'; }

let modelsLoaded=false;
async function refreshModels(){
  if(modelsLoaded) return;
  try{
    const models=await invoke('list_models');
    const existing=new Set([...document.querySelectorAll('.model-option')].map(o=>o.dataset.value));
    if(models?.length){
      for(const origin of ['local','cloud']){
        const group=models.filter(m=>m?.origin===origin && m?.tools && !existing.has(m.name));
        if(!group.length) continue;
        const heading=document.createElement('div'); heading.className='model-group-label'; heading.textContent=origin==='local'?'Local on this PC':'Cloud models';
        dynamicModels.appendChild(heading);
        for(const m of group){
          const b=document.createElement('button'); b.type='button'; b.className='model-option'; b.dataset.value=m.name;
          const flags=[origin==='local'?'Local':'Cloud',m.starter?'Starter':null,m.vision?'Vision':null,m.tools?'Tools':null].filter(Boolean).join(' · ');
          b.innerHTML=`<strong>${escapeHtml(m.label||m.name)}</strong><small>${escapeHtml(flags)}</small>`;
          b.addEventListener('click',()=>setSelectedModel(m.name,m.label||m.name));
          dynamicModels.appendChild(b); existing.add(m.name);
        }
      }
    }
    modelsLoaded=true;
    const chosen=[...document.querySelectorAll('.model-option')].find(b=>b.dataset.value===selectedModel);
    if(chosen) setSelectedModel(selectedModel, chosen.querySelector('strong')?.textContent || baseModels.get(selectedModel));
    else setSelectedModel('auto',baseModels.get('auto'));
  }catch(e){ console.warn('Could not load Ollama model list',e); }
}


async function refreshDashboard(){
  try{
    const [report,events]=await Promise.all([
      invoke('health_report'),
      invoke('proactive_events',{limit:8}).catch(()=>[])
    ]);
    const s=report.snapshot||{};
    const memPct=percent(s.memory_used_bytes,s.memory_total_bytes);
    $('#cpuMetric').textContent=`${Math.round(s.cpu_usage_percent||0)}%`;
    $('#cpuDetail').textContent=(s.cpu||'CPU').replace(/\s+/g,' ').slice(0,22);
    $('#memoryMetric').textContent=`${memPct}%`;
    $('#memoryDetail').textContent=`${formatBytes(s.memory_used_bytes)} / ${formatBytes(s.memory_total_bytes)}`;
    $('#storageMetric').textContent=s.root_available||'—';
    $('#storageDetail').textContent=`${report.disk_percent??'—'}% used on /`;
    const notice=$('#healthNotice');
    currentProactiveEvent=(events||[])[0]||null;
    if(currentProactiveEvent){
      notice.classList.remove('hidden');
      $('#healthNoticeTitle').textContent=currentProactiveEvent.title||'Fatir noticed something';
      $('#healthNoticeText').textContent=currentProactiveEvent.message||'';
      notice.dataset.prompt=currentProactiveEvent.action_prompt||'Show me what needs attention on this PC. Do not change anything yet.';
      notice.dataset.eventId=currentProactiveEvent.id||'';
    }else{
      const alerts=report.alerts||[];
      if(alerts.length){
        const a=alerts[0]; notice.classList.remove('hidden');
        $('#healthNoticeTitle').textContent=a.severity==='critical'?'System needs attention':'System heads-up';
        $('#healthNoticeText').textContent=alerts.map(x=>x.message).join(' · ');
        notice.dataset.prompt='Run a full health check on this PC. Explain the current alerts and recommend what to do. Do not change anything yet.';
        notice.dataset.eventId='';
      }else{
        notice.classList.add('hidden'); notice.dataset.prompt=''; notice.dataset.eventId='';
      }
    }
  }catch(e){
    ['cpuMetric','memoryMetric','storageMetric'].forEach(id=>$('#'+id).textContent='—');
    console.warn('Dashboard snapshot unavailable',e);
  }
}

function topNames(items=[]){
  return items.slice(0,4).map(x=>`${escapeHtml(x.name)} <b>${x.count}</b>`).join(' · ') || 'Not enough activity yet';
}
async function refreshActivity(){
  $('#activityList').innerHTML='<div class="activity-empty">Loading…</div>';
  try{
    const [actions,routine,obs,status]=await Promise.all([
      invoke('recent_actions',{limit:24}),
      invoke('routine_summary',{limit:900}),
      invoke('observer_status'),
      invoke('app_status')
    ]);
    const stats=obs.stats||{};
    $('#activityLearning').textContent=(obs.config?.enabled?'On':'Off');
    $('#activityActionsCount').textContent=String(actions.length||0);
    $('#activityConnection').textContent=status.connected?'Online':'Offline';
    if(!actions.length){
      $('#activityList').innerHTML='<div class="activity-empty">No Fatir actions recorded yet.</div>';
    }else{
      $('#activityList').innerHTML=actions.slice(0,18).map(a=>{
        const ok=(a.status||'')!=='error';
        const detail=String(a.result||'').replace(/\s+/g,' ').slice(0,110);
        return `<div class="activity-item ${ok?'':'error'}"><div class="activity-badge">${ok?'✓':'!'}</div><div class="activity-copy"><strong>${escapeHtml((a.tool||'action').replaceAll('_',' '))}</strong><small>${escapeHtml(detail||'Completed')}</small></div><div class="activity-time">${escapeHtml(formatWhen(a.at))}</div></div>`;
      }).join('');
    }
    $('#routineGrid').innerHTML=`
      <div class="routine-card"><span>Apps</span><p>${topNames(routine.top_apps)}</p></div>
      <div class="routine-card"><span>Sites</span><p>${topNames(routine.top_sites)}</p></div>
      <div class="routine-card wide"><span>Terminal</span><p>${topNames(routine.common_commands)}</p></div>`;
    $('#learningStats').textContent=`${stats.events||0} local events · ${stats.apps||0} app changes · ${stats.sites||0} web visits · ${stats.commands||0} commands`;
  }catch(e){
    $('#activityList').innerHTML=`<div class="activity-empty">Activity unavailable: ${escapeHtml(String(e))}</div>`;
  }
}

function taskStatusLabel(status='active'){ return String(status).replaceAll('_',' '); }
async function refreshRunBadge(){
  try{
    const jobs=await invoke('job_list');
    const running=(jobs||[]).filter(j=>j.status==='running').length;
    const badge=$('#runBadge');
    badge.textContent=`${running} running`; badge.classList.toggle('hidden',running===0);
  }catch{}
}
async function refreshOps(){
  try{
    const [tasks,jobs,undos,routines,cleanup,trash,alerts,projects,terminals,v1]=await Promise.all([
      invoke('task_list',{includeCompleted:false}),
      invoke('job_list'),
      invoke('rollback_list',{limit:60}),
      invoke('routine_list'),
      invoke('cleanup_scan',{path:'downloads',minAgeDays:14,limit:80}),
      invoke('trash_status',{limit:40}),
      invoke('proactive_events',{limit:40}),
      invoke('project_list'),
      invoke('terminal_session_list'),
      invoke('v1_status')
    ]);
    const running=(jobs||[]).filter(j=>j.status==='running').length;
    const available=(undos||[]).filter(u=>u.status==='available').length;
    $('#taskCount').textContent=String((tasks||[]).length);
    $('#jobCount').textContent=String(running);
    $('#routineCount').textContent=String((routines||[]).length);
    $('#projectCount').textContent=String((projects||[]).length);
    $('#alertCount').textContent=String((alerts||[]).length);
    $('#undoCount').textContent=String(available);
    const badge=$('#runBadge'); badge.textContent=`${running} running`; badge.classList.toggle('hidden',running===0);

    $('#taskList').innerHTML=(tasks||[]).length ? tasks.map(t=>`
      <div class="ops-item" data-task="${escapeHtml(t.id)}">
        <span class="ops-badge ${escapeHtml(t.status)}">${escapeHtml(taskStatusLabel(t.status))}</span>
        <div class="ops-copy"><strong>${escapeHtml(t.title)}</strong><small>${escapeHtml((t.phase||'understand').toUpperCase())} · ${escapeHtml(t.current_step||t.objective||'No current step')}${t.checkpoints?.length?` · ${escapeHtml(String(t.checkpoints.length))} checkpoint(s)`:''}</small>${t.last_error?`<small class="danger-text">${escapeHtml(t.last_error)}</small>`:''}</div>
        <div class="ops-actions"><button class="resume-task" type="button">Resume</button></div>
      </div>`).join('') : '<div class="activity-empty">No active tasks. Fatir creates one automatically for substantial multi-step work.</div>';
    $('#taskList').querySelectorAll('.resume-task').forEach(btn=>btn.addEventListener('click',()=>{
      const item=btn.closest('[data-task]'); const t=(tasks||[]).find(x=>x.id===item?.dataset.task); if(!t)return;
      composer.value=`Continue my Fatir task “${t.title}” (${t.id}). Review its persistent state, verify the current PC state, and resume from the next unfinished step.`;
      closeOps(); composer.focus(); autoSize();
    }));

    $('#jobList').innerHTML=(jobs||[]).length ? jobs.slice(0,40).map(j=>`
      <div class="ops-item" data-job="${escapeHtml(j.id)}">
        <span class="ops-badge ${escapeHtml(j.status)}">${escapeHtml(j.status)}</span>
        <div class="ops-copy"><strong>${escapeHtml(j.label||'Background job')}</strong><small>PID ${escapeHtml(String(j.pid||''))} · ${escapeHtml(formatWhen(j.created_at))}</small></div>
        <div class="ops-actions"><button class="job-log-btn" type="button">Log</button>${j.status==='running'?'<button class="job-stop-btn danger" type="button">Stop</button>':''}</div>
        <pre class="job-log"></pre>
      </div>`).join('') : '<div class="activity-empty">No background jobs yet.</div>';
    $('#jobList').querySelectorAll('.job-log-btn').forEach(btn=>btn.addEventListener('click',async()=>{
      const item=btn.closest('[data-job]'); const pre=item.querySelector('.job-log');
      try{pre.textContent=await invoke('job_log',{jobId:item.dataset.job,lines:160});pre.classList.toggle('open');}catch(e){addError(e)}
    }));
    $('#jobList').querySelectorAll('.job-stop-btn').forEach(btn=>btn.addEventListener('click',async()=>{
      const item=btn.closest('[data-job]'); if(!confirm('Stop this background job?'))return;
      try{await invoke('job_cancel',{jobId:item.dataset.job});await refreshOps();}catch(e){addError(e)}
    }));

    $('#routineListSaved').innerHTML=(routines||[]).length ? routines.map(r=>`
      <div class="ops-item stack" data-routine="${escapeHtml(r.id)}">
        <span class="ops-badge active">${escapeHtml(String(r.steps?.length||0))} steps</span>
        <div class="ops-copy"><strong>${escapeHtml(r.name)}</strong><small>${escapeHtml(r.description||'Reusable Fatir workflow')} · run ${escapeHtml(String(r.run_count||0))}×</small></div>
        <div class="ops-actions"><button class="routine-run" type="button">Run</button><button class="routine-remove danger" type="button">Remove</button></div>
      </div>`).join('') : '<div class="activity-empty">No saved routines yet. Ask Fatir to save a workflow, or capture recent successful actions.</div>';
    $('#routineListSaved').querySelectorAll('.routine-run').forEach(btn=>btn.addEventListener('click',()=>{
      const row=btn.closest('[data-routine]');const r=(routines||[]).find(x=>x.id===row?.dataset.routine);if(!r)return;
      closeOps();send(`Run my saved routine “${r.name}” (${r.id}). Load it with routine_prepare_run, verify the current state before each step, and keep all normal approval checks.`);
    }));
    $('#routineListSaved').querySelectorAll('.routine-remove').forEach(btn=>btn.addEventListener('click',async()=>{
      const row=btn.closest('[data-routine]');const r=(routines||[]).find(x=>x.id===row?.dataset.routine);if(!r||!confirm(`Remove routine “${r.name}”?`))return;
      try{await invoke('routine_remove',{id:r.id});await refreshOps();}catch(e){addError(e)}
    }));

    $('#projectList').innerHTML=(projects||[]).length ? projects.map(p=>`
      <div class="ops-item stack">
        <span class="ops-badge active">${escapeHtml((p.kind||[]).join(' / ')||'project')}</span>
        <div class="ops-copy"><strong>${escapeHtml(p.label||'Project')}</strong><small>${escapeHtml(p.path||'')} · ${escapeHtml(formatWhen(p.last_seen_at))}</small></div>
        <div class="ops-actions"><button class="project-open" data-path="${escapeHtml(p.path||'')}" type="button">Open</button><button class="project-work" data-label="${escapeHtml(p.label||'Project')}" data-path="${escapeHtml(p.path||'')}" type="button">Work</button></div>
      </div>`).join('') : '<div class="activity-empty">No remembered projects yet. Ask Fatir to remember a project folder after inspecting it.</div>';
    $('#projectList').querySelectorAll('.project-open').forEach(btn=>btn.addEventListener('click',()=>invoke('open_local_path',{path:btn.dataset.path}).catch(addError)));
    $('#projectList').querySelectorAll('.project-work').forEach(btn=>btn.addEventListener('click',()=>{closeOps();send(`Inspect and continue work on my project “${btn.dataset.label}” at ${btn.dataset.path}. Check the current project/Git state first and use a persistent terminal session if iterative shell work is needed.`)}));

    const teach=v1?.teach||{}; const engine=v1?.engine||{};
    $('#runtimeList').innerHTML=`
      <div class="ops-item stack"><span class="ops-badge active">V${escapeHtml(v1?.version||'1.0')}</span><div class="ops-copy"><strong>${escapeHtml(engine.engine||'Fatir V1 task engine')}</strong><small>Verification gate ${engine.verification_gate?'ON':'OFF'} · Recovery ${engine.recovery_classification?'ON':'OFF'} · Headless ${escapeHtml(engine.headless_policy||'explicit-only')}</small></div></div>
      <div class="ops-item stack"><span class="ops-badge ${teach.active?'waiting':'active'}">Teach 3.0</span><div class="ops-copy"><strong>${teach.active?escapeHtml(teach.name||'Recording'):'Not recording'}</strong><small>${escapeHtml(String(teach.steps||0))} recorded step(s)</small></div></div>
      <div class="ops-item stack"><span class="ops-badge active">Terminal</span><div class="ops-copy"><strong>${escapeHtml(String((terminals||[]).length))} persistent session(s)</strong><small>${(terminals||[]).slice(0,4).map(x=>`${escapeHtml(x.label)} · ${escapeHtml(x.cwd)}`).join('<br>')||'No persistent terminal sessions yet.'}</small></div></div>
      <div class="ops-item stack"><span class="ops-badge active">Schedules</span><div class="ops-copy"><strong>${escapeHtml(String(v1?.schedules||0))} persistent local schedule(s) · ${escapeHtml(String(v1?.agent_schedules||0))} agent schedule(s)</strong><small>systemd user timers · agent/web runs keep explicit browser modes and scoped credential authorization</small></div></div>
      <div class="ops-item stack"><span class="ops-badge ${((v1?.failures?.recent_errors||[]).length)?'blocked':'active'}">Recovery</span><div class="ops-copy"><strong>${escapeHtml(String((v1?.failures?.recent_errors||[]).length))} recent error sample(s)</strong><small>${(v1?.failures?.top_failed_tools||[]).slice(0,5).map(x=>`${escapeHtml(x[0])}: ${escapeHtml(String(x[1]))}`).join(' · ')||'No recent tool failures in the sampled history.'}</small></div></div>`;

    const candidates=cleanup?.candidates||[];
    $('#cleanupSummary').innerHTML=`
      <div class="cleanup-stat"><span>High confidence</span><strong>${escapeHtml(cleanup?.high_confidence_reclaimable||'0 B')}</strong></div>
      <div class="cleanup-stat"><span>Trash</span><strong>${escapeHtml(trash?.human||'0 B')}</strong></div>
      <div class="cleanup-stat"><span>Review items</span><strong>${escapeHtml(String(candidates.length||0))}</strong></div>`;
    $('#cleanupList').innerHTML=candidates.length ? candidates.slice(0,35).map(c=>`
      <div class="ops-item stack" data-cleanup-path="${escapeHtml(c.path)}">
        <span class="cleanup-confidence ${escapeHtml(c.confidence)}">${escapeHtml(c.confidence)}</span>
        <div class="ops-copy"><strong>${escapeHtml(c.name)} · ${escapeHtml(c.human)}</strong><small>${escapeHtml(c.reason)} · ${escapeHtml(String(c.age_days))}d old</small><button class="cleanup-path" type="button">${escapeHtml(c.path)}</button></div>
      </div>`).join('') : '<div class="activity-empty">No obvious Downloads cleanup candidates right now.</div>';
    $('#cleanupList').querySelectorAll('.cleanup-path').forEach(btn=>btn.addEventListener('click',()=>{
      const row=btn.closest('[data-cleanup-path]');if(row?.dataset.cleanupPath)invoke('open_local_path',{path:row.dataset.cleanupPath}).catch(addError);
    }));

    $('#alertList').innerHTML=(alerts||[]).length ? alerts.map(a=>`
      <div class="ops-item stack" data-alert="${escapeHtml(a.id)}">
        <span class="alert-severity ${escapeHtml(a.severity||'info')}"></span>
        <div class="ops-copy"><strong>${escapeHtml(a.title||'Fatir noticed something')}</strong><small>${escapeHtml(a.message||'')} · ${escapeHtml(formatWhen(a.created_at))}</small></div>
        <div class="ops-actions"><button class="alert-review" type="button">Review</button><button class="alert-dismiss" type="button">Dismiss</button></div>
      </div>`).join('') : '<div class="activity-empty">Nothing needs attention right now.</div>';
    $('#alertList').querySelectorAll('.alert-review').forEach(btn=>btn.addEventListener('click',async()=>{
      const row=btn.closest('[data-alert]');const a=(alerts||[]).find(x=>x.id===row?.dataset.alert);if(!a)return;
      try{await invoke('proactive_ack',{id:a.id});}catch{}
      closeOps();send(a.action_prompt||`Review this proactive event: ${a.title}. Do not make changes until needed.`);
    }));
    $('#alertList').querySelectorAll('.alert-dismiss').forEach(btn=>btn.addEventListener('click',async()=>{
      const row=btn.closest('[data-alert]');try{await invoke('proactive_ack',{id:row.dataset.alert});await refreshOps();await refreshDashboard();}catch(e){addError(e)}
    }));

    $('#undoList').innerHTML=(undos||[]).length ? undos.slice(0,50).map(u=>`
      <div class="ops-item" data-undo="${escapeHtml(u.id)}">
        <span class="ops-badge ${u.status==='available'?'active':''}">${escapeHtml(u.status)}</span>
        <div class="ops-copy"><strong>${escapeHtml(u.label||'Rollback point')}</strong><small>${escapeHtml(formatWhen(u.at))} · ${escapeHtml((u.kind||'').replaceAll('_',' '))}</small></div>
        <div class="ops-actions">${u.status==='available'?'<button class="undo-btn" type="button">Undo</button>':''}</div>
      </div>`).join('') : '<div class="activity-empty">No rollback points yet. Fatir creates them before supported changes.</div>';
    $('#undoList').querySelectorAll('.undo-btn').forEach(btn=>btn.addEventListener('click',async()=>{
      const item=btn.closest('[data-undo]'); if(!confirm('Restore this rollback point? This changes the current system/file state.'))return;
      try{await invoke('rollback_execute',{id:item.dataset.undo});await refreshOps();addNotice('Rollback completed.');}catch(e){addError(e)}
    }));

    $('#captureRoutine').onclick=()=>{closeOps();composer.value='Turn my recent successful Fatir actions into a reusable routine. First summarize what you would capture, exclude any credential/sensitive actions, then ask me for the routine name if needed.';composer.focus();autoSize()};
    $('#reviewCleanup').onclick=()=>{closeOps();send('Review my current Downloads cleanup candidates. Explain high-confidence, uncertain and keep items. Do not delete anything until I approve specific paths.')};
    $('#scanDuplicates').onclick=async()=>{
      const holder=$('#cleanupList');holder.innerHTML='<div class="activity-empty">Hashing likely duplicate files…</div>';
      try{
        const dup=await invoke('cleanup_duplicates',{path:'downloads',minimumSizeMb:5,limitGroups:30});
        const groups=dup.groups||[];
        holder.innerHTML=groups.length?groups.map(g=>`<div class="ops-item stack"><span class="cleanup-confidence high">${escapeHtml(String(g.copies))} copies</span><div class="ops-copy"><strong>${escapeHtml(g.reclaimable||'')} reclaimable</strong><small>${(g.paths||[]).map(p=>escapeHtml(p)).join('<br>')}</small></div></div>`).join(''):'<div class="activity-empty">No byte-for-byte duplicate groups found in Downloads.</div>';
        $('#cleanupSummary').innerHTML=`<div class="cleanup-stat"><span>Duplicate reclaimable</span><strong>${escapeHtml(dup.reclaimable||'0 B')}</strong></div><div class="cleanup-stat"><span>Groups</span><strong>${escapeHtml(String(groups.length))}</strong></div><div class="cleanup-stat"><span>Method</span><strong>SHA-256</strong></div>`;
      }catch(e){holder.innerHTML=`<div class="activity-empty">Duplicate scan failed: ${escapeHtml(String(e))}</div>`}
    };
  }catch(e){
    $('#taskList').innerHTML=`<div class="activity-empty">Command center unavailable: ${escapeHtml(String(e))}</div>`;
  }
}

async function openOps(){ opsSheet.classList.remove('hidden');opsSheet.setAttribute('aria-hidden','false');await refreshOps(); }
function closeOps(){ opsSheet.classList.add('hidden');opsSheet.setAttribute('aria-hidden','true'); }

async function refreshCredentials(){
  const holder=$('#credentialList'); holder.innerHTML='<div class="activity-empty">Loading keyring metadata…</div>';
  try{
    const items=await invoke('credential_list');
    holder.innerHTML=items.length?items.map(c=>`<div class="credential-item" data-cred="${escapeHtml(c.id)}"><span class="credential-lock">⌾</span><div class="credential-copy"><strong>${escapeHtml(c.label)}</strong><small>${escapeHtml(c.account||'No account label')}</small></div><button class="credential-remove" type="button">Remove</button></div>`).join(''):'<div class="activity-empty">No stored credentials. Add one above when you want Fatir to use a secret without exposing it to a model.</div>';
    holder.querySelectorAll('.credential-remove').forEach(btn=>btn.addEventListener('click',async()=>{const row=btn.closest('[data-cred]');if(!confirm('Remove this credential from the Linux keyring?'))return;try{await invoke('credential_remove',{id:row.dataset.cred});await refreshCredentials();}catch(e){addError(e)}}));
  }catch(e){holder.innerHTML=`<div class="activity-empty">Credential metadata unavailable: ${escapeHtml(String(e))}</div>`}
}
async function openActivity(){
  activitySheet.classList.remove('hidden'); activitySheet.setAttribute('aria-hidden','false');
  await refreshActivity();
}
function closeActivity(){ activitySheet.classList.add('hidden'); activitySheet.setAttribute('aria-hidden','true'); }
function newChat(){
  if(busy) return;
  sessionId=crypto.randomUUID(); localStorage.setItem('fatir-session',sessionId);
  messages.innerHTML=''; clearAttachments(); welcome.classList.remove('hidden');
  composer.value=''; autoSize(); composer.focus(); refreshDashboard();
}
async function loadChatSession(id,{closeSheet=true}={}){
  if(busy) return;
  try{
    const history=await invoke('chat_history',{sessionId:id});
    sessionId=id;
    localStorage.setItem('fatir-session',sessionId);
    messages.innerHTML='';
    clearAttachments();
    const rows=history.messages||[];
    if(rows.length) welcome.classList.add('hidden'); else welcome.classList.remove('hidden');
    for(const row of rows){
      const text=row.content||'';
      if(row.role==='user'){
        const el=document.createElement('div'); el.className='message user';
        el.innerHTML=`<div class="bubble">${formatText(text)}</div>`;
        messages.appendChild(el); wireRichContent(el);
      }else if(row.role==='assistant'){
        const el=document.createElement('div'); el.className='message assistant';
        el.innerHTML=`<div class="assistant-label">Fatir <span class="model-tag">History</span></div><div class="bubble">${formatText(text)}</div>`;
        messages.appendChild(el); wireRichContent(el);
      }
    }
    if(closeSheet) closeChats();
    scrollBottom();
    composer.focus();
  }catch(e){
    if(!String(e).toLowerCase().includes('chat not found')) addError(e);
  }
}

async function restoreCurrentChat(){
  await loadChatSession(sessionId,{closeSheet:false});
}

async function refreshChats(){
  const holder=$('#chatHistoryList');
  if(!holder) return;
  holder.innerHTML='<div class="activity-empty">Loading chats…</div>';
  try{
    const chats=await invoke('chat_list');
    if(!(chats||[]).length){
      holder.innerHTML='<div class="activity-empty">No saved conversations yet.</div>';
      return;
    }
    holder.innerHTML=chats.slice(0,80).map(chat=>`
      <button type="button" class="chat-history-row" data-session="${escapeHtml(chat.session_id)}">
        <span class="chat-history-icon">${chat.source==='schedule'?'⌚':chat.source==='companion'?'◫':'◇'}</span>
        <span class="chat-history-copy">
          <strong>${escapeHtml(chat.title||'Chat')}</strong>
          <small>${escapeHtml(chat.preview||'')}${chat.message_count?` · ${chat.message_count} messages`:''}</small>
        </span>
        <span class="chat-history-source">${escapeHtml(chat.source||'desktop')}</span>
      </button>`).join('');
    holder.querySelectorAll('.chat-history-row').forEach(btn=>btn.addEventListener('click',()=>loadChatSession(btn.dataset.session)));
  }catch(e){
    holder.innerHTML=`<div class="activity-empty">Chat history unavailable: ${escapeHtml(String(e))}</div>`;
  }
}
async function openChats(){
  $('#chatsSheet')?.classList.remove('hidden');
  $('#chatsSheet')?.setAttribute('aria-hidden','false');
  await refreshChats();
}
function closeChats(){
  $('#chatsSheet')?.classList.add('hidden');
  $('#chatsSheet')?.setAttribute('aria-hidden','true');
}

async function refreshPin(){
  try{
    const s=await invoke('panel_state');
    pinBtn.classList.toggle('pinned',!!s.pinned);
    pinBtn.textContent=s.pinned?'◆':'◇';
    pinBtn.title=s.pinned?'Unpin Fatir':'Keep Fatir open';
  }catch(e){console.warn(e)}
}
async function togglePin(){
  try{
    const current=pinBtn.classList.contains('pinned');
    const s=await invoke('set_panel_pinned',{pinned:!current});
    pinBtn.classList.toggle('pinned',!!s.pinned);
    pinBtn.textContent=s.pinned?'◆':'◇';
    pinBtn.title=s.pinned?'Unpin Fatir':'Keep Fatir open';
  }catch(e){addError(e)}
}

function preferredPanelLayout(){
  const side=(localStorage.getItem('fatir-panel-side')||'right')==='left'?'left':'right';
  const raw=Number(localStorage.getItem('fatir-panel-width')||420);
  const width=Math.max(360,Math.min(520,Number.isFinite(raw)?raw:420));
  return {side,width};
}
function updatePanelControls(side,width){
  if(panelSide) panelSide.value=side;
  if(panelWidth) panelWidth.value=String(width);
  if(panelWidthValue) panelWidthValue.textContent=`${width} px`;
}
async function applyPanelLayout({persist=true}={}){
  const current=preferredPanelLayout();
  const side=panelSide?.value==='left'?'left':current.side;
  const width=Math.max(360,Math.min(520,Number(panelWidth?.value||current.width)||420));
  if(persist){localStorage.setItem('fatir-panel-side',side);localStorage.setItem('fatir-panel-width',String(width));}
  updatePanelControls(side,width);
  try{return await invoke('set_panel_layout',{width,side,mode:'open'});}catch(e){console.warn('Panel layout unavailable',e);}
}
async function refreshPanelLayout(){
  const pref=preferredPanelLayout();
  updatePanelControls(pref.side,pref.width);
  try{
    const state=await invoke('panel_state');
    const storedSide=localStorage.getItem('fatir-panel-side');
    const storedWidth=localStorage.getItem('fatir-panel-width');
    if(!storedSide&&state?.side) localStorage.setItem('fatir-panel-side',state.side);
    if(!storedWidth&&state?.width) localStorage.setItem('fatir-panel-width',String(state.width));
    const next=preferredPanelLayout(); updatePanelControls(next.side,next.width);
    await applyPanelLayout({persist:false});
  }catch(e){console.warn(e)}
}

async function refreshComputerControl(){
  try{
    const s=await invoke('computer_control_state');
    const paused=!!s.paused;
    computerControlBtn?.classList.toggle('paused',paused);
    if(computerControlBtn){
      computerControlBtn.textContent=paused?'▶':'⌖';
      computerControlBtn.title=paused?'Resume Fatir computer control':'Fatir computer control active — click to take over';
      computerControlBtn.setAttribute('aria-label',paused?'Resume Fatir computer control':'Pause Fatir computer control and take over');
    }
  }catch(e){console.warn('Computer control state unavailable',e)}
}
async function toggleComputerControl(){
  try{
    const current=await invoke('computer_control_state');
    const next=await invoke('set_computer_control_paused',{paused:!current.paused});
    await refreshComputerControl();
    addNotice(next.paused?'Your turn — Fatir computer control paused.':'Fatir control resumed — its virtual pointer can act again.');
  }catch(e){addError(e)}
}

async function refreshStatus(){
  try{
    const s=await invoke('app_status');
    statusDot.className='status-dot '+(s.connected?'online':'offline');
    statusText.textContent=s.connected?s.connection:'Needs connection';
    $('#connectionDetail').textContent=s.connection;
    if(s.connected) refreshModels();
    return s;
  }catch(e){
    statusDot.className='status-dot offline'; statusText.textContent='Offline'; $('#connectionDetail').textContent=String(e);
  }
}

modelButton.addEventListener('click',e=>{
  e.stopPropagation();
  const open=modelMenu.classList.toggle('hidden')===false;
  modelButton.setAttribute('aria-expanded',String(open));
});
modelMenu.addEventListener('click',e=>e.stopPropagation());
document.addEventListener('click',()=>{ modelMenu.classList.add('hidden'); modelButton.setAttribute('aria-expanded','false'); });
document.querySelectorAll('.model-option').forEach(b=>b.addEventListener('click',()=>setSelectedModel(b.dataset.value,b.querySelector('strong')?.textContent||baseModels.get(b.dataset.value))));

sendBtn.onclick=()=>busy?stopFlow():send();
$('#attachBtn').onclick=addFiles;
$('#shotBtn').onclick=takeShot;
composer.addEventListener('input',autoSize);
composer.addEventListener('keydown',e=>{ if(e.key==='Enter'&&!e.shiftKey){e.preventDefault(); if(!busy) send();} });
document.querySelectorAll('.quick[data-prompt]').forEach(b=>b.onclick=()=>send(b.dataset.prompt));
document.querySelectorAll('.quick[data-prefill]').forEach(b=>b.onclick=()=>{composer.value=b.dataset.prefill;composer.focus();autoSize()});
$('#quickRead').onclick=async()=>{ await addFiles(); if(attachments.length){composer.value='Open the attached file and tell me clearly what it contains.';composer.focus();autoSize()} };
$('#shareBtn')?.addEventListener('click',()=>openShare());
$('#quickShareShot')?.addEventListener('click',async()=>{try{const f=await invoke('share_latest_screenshot');await openShare([f.path]);}catch(e){addError(e)}});
$('#closeShare')?.addEventListener('click',closeShare);
shareSheet?.addEventListener('click',e=>{if(e.target===shareSheet)closeShare()});
document.querySelectorAll('.share-back').forEach(b=>b.addEventListener('click',()=>showSharePanel('home')));
$('#shareWhatsApp')?.addEventListener('click',loadWhatsAppContacts);
$('#refreshWhatsApp')?.addEventListener('click',loadWhatsAppContacts);
$('#whatsappSearch')?.addEventListener('input',()=>renderWhatsAppContacts(whatsappAllContacts));
$('#sendWhatsAppManual')?.addEventListener('click',()=>sendWhatsApp($('#whatsappManual').value));
$('#whatsappManual')?.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();sendWhatsApp(e.currentTarget.value)}});
$('#shareEmail')?.addEventListener('click',()=>showSharePanel('email'));
$('#createEmailDraft')?.addEventListener('click',createEmailDraft);
$('#shareCopy')?.addEventListener('click',async()=>{try{await invoke('share_copy',{paths:sharePaths});addNotice('Copied to clipboard.');closeShare()}catch(e){shareSetStatus(String(e),'error')}});
$('#shareCopyPath')?.addEventListener('click',async()=>{try{await invoke('share_copy_paths',{paths:sharePaths});addNotice('Path copied to clipboard.');closeShare()}catch(e){shareSetStatus(String(e),'error')}});

function shareSetStatus(text='',kind='info'){
  const el=$('#shareStatus'); if(!el)return;
  el.textContent=text; el.className=`share-status ${text?'':'hidden'} ${kind}`;
}
function showSharePanel(which='home'){
  $('#shareHome')?.classList.toggle('hidden',which!=='home');
  $('#shareWhatsAppPanel')?.classList.toggle('hidden',which!=='whatsapp');
  $('#shareEmailPanel')?.classList.toggle('hidden',which!=='email');
  shareSetStatus('');
}
async function closeShare(){
  shareSheet?.classList.add('hidden'); shareSheet?.setAttribute('aria-hidden','true'); showSharePanel('home');
  if(shareAutoPinned){shareAutoPinned=false;try{await invoke('set_panel_pinned',{pinned:false});await refreshPin();}catch{}}
}
function renderShareFiles(){
  const holder=$('#shareFileList'); if(!holder)return;
  holder.innerHTML=shareFiles.length?shareFiles.map(f=>`<button type="button" class="share-file" data-share-path="${escapeHtml(f.path)}" title="Open ${escapeHtml(f.path)}"><span class="share-file-icon">${f.mime?.startsWith('image/')?'IMG':'FILE'}</span><span><strong>${escapeHtml(f.name)}</strong><small>${escapeHtml(formatBytes(f.bytes))} · ${escapeHtml(f.mime||'file')}</small></span></button>`).join(''):'<div class="activity-empty">Choose a file to share.</div>';
  holder.querySelectorAll('.share-file').forEach(b=>b.onclick=()=>invoke('open_local_path',{path:b.dataset.sharePath}).catch(addError));
}
async function refreshShareHistory(){
  try{const rows=await invoke('share_history',{limit:6});const h=$('#shareHistory');if(!h)return;h.innerHTML=rows?.length?rows.map(x=>`<div class="share-history-item"><span>${x.destination==='whatsapp'?'W':'@'}</span><div><strong>${escapeHtml(x.recipient||x.destination)}</strong><small>${escapeHtml((x.paths||[]).map(p=>p.split('/').pop()).join(', '))}</small></div><time>${escapeHtml(formatWhen(x.timestamp))}</time></div>`).join(''):'<div class="activity-empty">No recent shares.</div>';}catch(e){console.warn(e)}
}
async function openShare(paths=[]){
  try{
    let chosen=(paths||[]).filter(Boolean);
    if(!chosen.length) chosen=await invoke('pick_files');
    if(!chosen?.length)return;
    shareFiles=await invoke('share_describe',{paths:chosen});
    sharePaths=shareFiles.map(x=>x.path);
    try{const p=await invoke('panel_state');if(!p.pinned){await invoke('set_panel_pinned',{pinned:true});shareAutoPinned=true;await refreshPin();}}catch{}
    renderShareFiles(); showSharePanel('home'); refreshShareHistory();
    shareSheet.classList.remove('hidden');shareSheet.setAttribute('aria-hidden','false');
  }catch(e){addError(e)}
}
function renderWhatsAppContacts(items){
  const q=($('#whatsappSearch')?.value||'').trim().toLowerCase();
  const filtered=(items||[]).filter(x=>!q||x.toLowerCase().includes(q)).slice(0,60);
  const h=$('#whatsappContacts'); if(!h)return;
  h.innerHTML=filtered.length?filtered.map(name=>`<button type="button" class="contact-item" data-contact="${escapeHtml(name)}"><span class="contact-avatar">${escapeHtml(name.slice(0,1).toUpperCase())}</span><strong>${escapeHtml(name)}</strong><span>→</span></button>`).join(''):'<div class="activity-empty">No matching recent chats. Type a contact name below.</div>';
  h.querySelectorAll('.contact-item').forEach(b=>b.onclick=()=>sendWhatsApp(b.dataset.contact));
}
async function loadWhatsAppContacts(){
  showSharePanel('whatsapp'); $('#whatsappContacts').innerHTML='<div class="activity-empty">Reading recent chats from Fatir browser…</div>';
  try{const r=await invoke('share_whatsapp_contacts');whatsappAllContacts=r.contacts||[];renderWhatsAppContacts(whatsappAllContacts);if(r.warning)shareSetStatus('Showing cached contacts. Open/sign in to WhatsApp Web to refresh.','warn');}
  catch(e){$('#whatsappContacts').innerHTML=`<div class="activity-empty">${escapeHtml(String(e))}</div>`;shareSetStatus(String(e),'error')}
}
async function sendWhatsApp(contact){
  contact=String(contact||'').trim(); if(!contact)return;
  shareSetStatus(`Sending ${sharePaths.length} file${sharePaths.length===1?'':'s'} to ${contact}…`,'working');
  try{await invoke('share_whatsapp_send',{paths:sharePaths,contact});shareSetStatus(`Sent to ${contact}.`,'success');await refreshShareHistory();setTimeout(closeShare,900)}catch(e){shareSetStatus(String(e),'error')}
}
async function createEmailDraft(){
  const to=$('#shareEmailTo').value.trim(),subject=$('#shareEmailSubject').value.trim();
  if(!to){shareSetStatus('Enter an email address.','error');return;}
  shareSetStatus(`Creating Gmail draft for ${to}…`,'working');
  try{await invoke('share_email_draft',{paths:sharePaths,to,subject});shareSetStatus('Gmail draft is open with the files attached. Review it in the Fatir browser before sending.','success');await refreshShareHistory();}catch(e){shareSetStatus(String(e),'error')}
}
async function checkPendingShare(){
  try{const paths=await invoke('share_pending');if(paths?.length)await openShare(paths);}catch{}
}

async function refreshObserver(){
  try{
    const s=await invoke('observer_status');
    const c=s.config||{};
    $('#learnEnabled').checked=!!c.enabled;
    $('#learnApps').checked=!!c.active_apps;
    $('#learnWeb').checked=!!c.browser_history;
    $('#learnTerminal').checked=!!c.terminal_history;
    const st=s.stats||{};
    $('#learningStats').textContent=`${st.events||0} local events · ${st.apps||0} app changes · ${st.sites||0} web visits · ${st.commands||0} commands`;
  }catch(e){ $('#learningStats').textContent='Learning status unavailable'; console.warn(e); }
}
async function saveObserver(){
  const config={enabled:$('#learnEnabled').checked,active_apps:$('#learnApps').checked,browser_history:$('#learnWeb').checked,terminal_history:$('#learnTerminal').checked,sample_seconds:8};
  try{await invoke('set_observer_config',{config});await refreshObserver();}catch(e){addError(e)}
}
['learnEnabled','learnApps','learnWeb','learnTerminal'].forEach(id=>$('#'+id)?.addEventListener('change',saveObserver));
$('#clearLearning')?.addEventListener('click',async()=>{try{await invoke('clear_observer_data');await refreshObserver();}catch(e){addError(e)}});

async function refreshAdaptive(){
  try{
    const s=await invoke('adaptive_status'); const c=s.config||{}, st=s.stats||{};
    $('#adaptiveEnabled').checked=!!c.enabled;
    $('#adaptiveLocalFirst').checked=!!c.local_first;
    $('#adaptiveRoutes').checked=!!c.learn_routes;
    $('#adaptiveRoutines').checked=!!c.suggest_routines;
    $('#adaptiveStats').textContent=`${st.runs||0} learned runs · ${st.local||0} local · ${st.cloud||0} cloud · ${st.fast_path||0} fast path · ${st.success_rate||0}% success`;
    const items=s.suggestions||[];
    $('#adaptiveSuggestions').innerHTML=items.length?items.map(x=>`<div class="adaptive-card"><strong>${escapeHtml(x.title||'Learned improvement')}</strong><small>${escapeHtml(x.detail||'')}</small></div>`).join(''):'<div class="activity-empty">Fatir will surface routing and routine improvements after it has enough successful runs.</div>';
  }catch(e){ $('#adaptiveStats').textContent='Adaptive learning unavailable'; console.warn(e); }
}
async function saveAdaptive(){
  const config={enabled:$('#adaptiveEnabled').checked,local_first:$('#adaptiveLocalFirst').checked,learn_routes:$('#adaptiveRoutes').checked,suggest_routines:$('#adaptiveRoutines').checked};
  try{await invoke('set_adaptive_config',{config});await refreshAdaptive();}catch(e){addError(e)}
}
['adaptiveEnabled','adaptiveLocalFirst','adaptiveRoutes','adaptiveRoutines'].forEach(id=>$('#'+id)?.addEventListener('change',saveAdaptive));
$('#clearAdaptive')?.addEventListener('click',async()=>{try{await invoke('clear_adaptive_learning');await refreshAdaptive();}catch(e){addError(e)}});

async function refreshProactiveSettings(){
  try{
    const s=await invoke('proactive_status');
    const c=s.config||{}; proactiveConfigCache=c;
    $('#proactiveEnabled').checked=!!c.enabled;
    $('#proactiveNotify').checked=!!c.desktop_notifications;
    $('#proactiveStats').textContent=`${s.unacknowledged||0} unacknowledged · checks every ${c.scan_minutes||15} min`;
  }catch(e){$('#proactiveStats').textContent='Proactive status unavailable';console.warn(e)}
}
async function saveProactiveSettings(){
  const base=proactiveConfigCache||{disk_warning_percent:85,trash_warning_mb:1024,downloads_warning_mb:1536,scan_minutes:15};
  const config={...base,enabled:$('#proactiveEnabled').checked,desktop_notifications:$('#proactiveNotify').checked};
  try{await invoke('set_proactive_config',{config});proactiveConfigCache=config;await refreshProactiveSettings();}catch(e){addError(e)}
}
['proactiveEnabled','proactiveNotify'].forEach(id=>$('#'+id)?.addEventListener('change',saveProactiveSettings));
$('#checkProactive')?.addEventListener('click',async()=>{
  try{await invoke('proactive_check_now');await refreshProactiveSettings();await refreshDashboard();addNotice('Proactive check completed.');}catch(e){addError(e)}
});


$('#systemStrip')?.addEventListener('click',refreshDashboard);
closePanelBtn?.addEventListener('click',()=>invoke('hide_panel').catch(addError));
panelSide?.addEventListener('change',()=>applyPanelLayout());
panelWidth?.addEventListener('input',()=>{
  const width=Math.max(360,Math.min(520,Number(panelWidth.value)||420));
  if(panelWidthValue)panelWidthValue.textContent=`${width} px`;
  clearTimeout(panelLayoutTimer);
  panelLayoutTimer=setTimeout(()=>applyPanelLayout(),90);
});
computerControlBtn?.addEventListener('click',toggleComputerControl);
pinBtn?.addEventListener('click',togglePin);
$('#activityBtn')?.addEventListener('click',openActivity);
$('#closeActivity')?.addEventListener('click',closeActivity);
$('#refreshActivity')?.addEventListener('click',refreshActivity);
activitySheet?.addEventListener('click',e=>{if(e.target===activitySheet) closeActivity();});
$('#newChatBtn')?.addEventListener('click',newChat);
inlineStop?.addEventListener('click',stopFlow);
document.addEventListener('keydown',e=>{
  if(e.key==='Escape'){
    modelMenu.classList.add('hidden');
    if(!activitySheet.classList.contains('hidden')) closeActivity();
    if(!opsSheet.classList.contains('hidden')) closeOps();
    if(!$('#settingsModal').classList.contains('hidden')) $('#settingsModal').classList.add('hidden');
    if(shareSheet && !shareSheet.classList.contains('hidden')) closeShare();
  }
  if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='n'){e.preventDefault();newChat();}
});

const modal=$('#settingsModal');
async function refreshPermissions(){
  try{
    const p=await invoke('permission_status'); const c=p.config||{};
    $('#confirmFileChanges').checked=!!c.confirm_file_changes;
    $('#confirmShellCommands').checked=!!c.confirm_shell_commands;
    $('#confirmBackgroundJobs').checked=!!c.confirm_background_jobs;
    $('#confirmAutomationMetadata').checked=!!c.confirm_automation_metadata;
    $('#permissionStats').textContent='Destructive, administrator and credential actions always ask.';
  }catch(e){ $('#permissionStats').textContent=`Permission policy unavailable: ${String(e)}`; }
}
async function savePermissions(){
  const config={confirm_file_changes:$('#confirmFileChanges').checked,confirm_shell_commands:$('#confirmShellCommands').checked,confirm_background_jobs:$('#confirmBackgroundJobs').checked,confirm_automation_metadata:$('#confirmAutomationMetadata').checked};
  try{await invoke('set_permission_config',{config});await refreshPermissions();}catch(e){addError(e)}
}
['confirmFileChanges','confirmShellCommands','confirmBackgroundJobs','confirmAutomationMetadata'].forEach(id=>$('#'+id)?.addEventListener('change',savePermissions));

$('#settingsBtn').onclick=()=>{modal.classList.remove('hidden');refreshPanelLayout();refreshStatus();refreshObserver();refreshAdaptive();refreshCredentials();refreshProactiveSettings();refreshPermissions()};
$('#closeSettings').onclick=()=>modal.classList.add('hidden');
modal.addEventListener('click',e=>{if(e.target===modal)modal.classList.add('hidden')});
$('#saveKey').onclick=async()=>{const key=$('#apiKey').value.trim();if(!key)return;try{await invoke('save_api_key',{key});$('#apiKey').value='';await refreshStatus()}catch(e){addError(e)}};
$('#clearKey').onclick=async()=>{try{await invoke('clear_api_key');await refreshStatus()}catch(e){addError(e)}};
$('#testConnection').onclick=refreshStatus;
$('#openData').onclick=()=>invoke('reveal_data_dir');


$('#chatsBtn')?.addEventListener('click',openChats);
$('#closeChats')?.addEventListener('click',closeChats);
$('#refreshChats')?.addEventListener('click',refreshChats);
$('#chatsSheet')?.addEventListener('click',e=>{if(e.target===$('#chatsSheet'))closeChats();});

$('#opsBtn')?.addEventListener('click',openOps);
$('#closeOps')?.addEventListener('click',closeOps);
opsSheet?.addEventListener('click',e=>{if(e.target===opsSheet)closeOps();});
document.querySelectorAll('.ops-tab').forEach(btn=>btn.addEventListener('click',()=>{
  document.querySelectorAll('.ops-tab').forEach(x=>x.classList.toggle('active',x===btn));
  const tab=btn.dataset.tab; const panels={tasks:'#opsTasks',jobs:'#opsJobs',routines:'#opsRoutines',projects:'#opsProjects',runtime:'#opsRuntime',cleanup:'#opsCleanup',alerts:'#opsAlerts',undo:'#opsUndo'}; Object.entries(panels).forEach(([name,sel])=>$(sel)?.classList.toggle('hidden',tab!==name));
}));
$('#healthNoticeAction')?.addEventListener('click',async()=>{const notice=$('#healthNotice');const prompt=notice?.dataset.prompt||'Run a full health check on this PC. Explain what needs attention and do not change anything yet.';const id=notice?.dataset.eventId;if(id){try{await invoke('proactive_ack',{id});}catch{}}send(prompt);});
$('#saveCredential')?.addEventListener('click',async()=>{
  const label=$('#credLabel').value.trim(),account=$('#credAccount').value.trim(),secret=$('#credSecret').value;
  if(!label||!secret){addNotice('Credential label and secret are required.');return;}
  try{await invoke('credential_store',{label,account,secret});$('#credLabel').value='';$('#credAccount').value='';$('#credSecret').value='';await refreshCredentials();addNotice('Credential stored in Linux keyring.');}catch(e){addError(e)}
});
setInterval(refreshRunBadge,15000);
setInterval(refreshDashboard,60000);

setGreeting();
setSelectedModel(selectedModel,baseModels.get(selectedModel)||selectedModel);
refreshPanelLayout();
refreshStatus();
refreshObserver();
refreshAdaptive();
refreshDashboard();
refreshRunBadge();
refreshPin();
refreshComputerControl();
restoreCurrentChat();
checkPendingShare();
setInterval(checkPendingShare,1200);
composer.focus();
