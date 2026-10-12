import assert from 'node:assert/strict';
import { test } from 'node:test';
import http from 'node:http';
import { createRunner } from '../../clients/npm/antikythera-sdk/component/instantiate.js';
import runtimeApi from '../../clients/npm/antikythera-sdk/runtime/index.js';
import orchestratorApi from '../../clients/npm/antikythera-sdk/orchestrator.js';

test('jco Component Model instances isolate hooks, sessions, and tool registries', async () => {
  const a = await createRunner({ decideAction: () => ({ action: 'final', content: 'A' }) });
  const b = await createRunner({ decideAction: () => ({ action: 'final', content: 'B' }) });
  const sa = a.init(JSON.stringify({ session_id: 'same-id', runtime_hooks_enabled: true }));
  const sb = b.init(JSON.stringify({ session_id: 'same-id', runtime_hooks_enabled: true }));
  a.registerTools(JSON.stringify([{ name: 'privateA', description: 'A tool', input_schema: {type:'object'}, parameters:[], title:null, output_schema:null }]));
  assert.match(a.getToolsPrompt(), /privateA/);
  assert.doesNotMatch(b.getToolsPrompt(), /privateA/);
  for (const [runner, id, expected] of [[a, sa, 'A'], [b, sb, 'B'], [a, sa, 'A']]) {
    const prepared = runner.prepareUserTurn(JSON.stringify({prompt:'hello', session_id:id}));
    assert.equal(JSON.parse(runner.commitLlmResponse(prepared, '{"action":"final","content":"provider"}')).content, expected);
  }
  a.resetSession(sa);
  assert.equal(JSON.parse(b.getState(sb)).session_id, sb);
});

test('concurrent orchestrator tasks reserve task and step budgets before execution', async () => {
  let calls = 0;
  const factory = async () => ({ connect:async()=>{}, runTurn:async (_prompt, opts) => {
    calls++;
    assert.equal(opts.maxSteps, 1);
    await new Promise(resolve => setTimeout(resolve, 25));
    return {sessionId:'s', action:'final', content:'ok', iterations:1};
  }});
  for (const limit of [{maxTotalTasks:1, maxTotalSteps:1}, {maxTotalSteps:1}]) {
    calls=0;
    const orch = new orchestratorApi.Orchestrator({serverUrl:'http://localhost', executionMode:'concurrent', runtimeFactory:factory, ...limit});
    orch.registerAgent({id:'a', name:'A', role:'worker'});
    const results=await orch.dispatchMany(['one','two','three']);
    assert.equal(calls,1);
    assert.equal(results.filter(result=>result.success).length,1);
    assert.equal(orch.getBudget().consumedSteps,1);
    orch.close();
  }
});

test('partial SSE cannot replace authoritative content; cancellation aborts pending HTTP', async () => {
  let pending;
  let sse;
  const server = http.createServer((req,res) => {
    if (req.url.includes('/tools')) {res.setHeader('Content-Type','application/json');res.end('[]');}
    else if (req.url.includes('/events')) {sse = res; res.writeHead(200,{'Content-Type':'text/event-stream'});res.write(': ready\n\n');}
    else if (req.url.includes('/llm/call')) {
      if (pending) {pending(); return;}
      sse.write('data: '+JSON.stringify({type:'llm-token',payload:{session_id:'regression-session',chunk:'partial'}})+'\n\n');
      res.setHeader('Content-Type','application/json');
      res.end(JSON.stringify({content:'{"action":"final","content":"complete"}',message_json:'{"content":"wrong-wrapper"}',model:null,session_id:null,tokens_used:null,finish_reason:'stop',raw_response_json:null}));
    } else res.end();
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const runtime=await runtimeApi.createAgentRuntime({sessionId:'regression-session',serverUrl:`http://127.0.0.1:${server.address().port}`});
  try {
    await runtime.connect();
    await assert.rejects(runtime.runTurn('invalid limit', { maxSteps: NaN }), /non-negative integer/);
    assert.equal((await runtime.runTurn('first')).content,'complete');
    const requested = new Promise(resolve=>{pending=resolve;});
    const running=runtime.runTurn('cancel me');
    await requested;
    assert.equal(runtime.cancelTurn(),true);
    await assert.rejects(running, error=>error.stepsUsed===1 && /cancelled/.test(error.message));
  } finally { runtime.close(); server.closeAllConnections(); await new Promise(resolve=>server.close(resolve)); }
});

test('server-core connection closes a pending SSE without leaking the legacy hook slot', async () => {
  let requested;
  const requestArrived = new Promise(resolve => { requested = resolve; });
  const server = http.createServer((_req, _res) => requested());
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const previous = globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__;
  const runtime = await runtimeApi.createAgentRuntime({
    core: 'server',
    serverUrl: `http://127.0.0.1:${server.address().port}`,
    hooks: { decideAction: () => ({ action: 'final', content: 'private' }) },
  });
  try {
    assert.equal(globalThis.__ANTIKYTHERA_RUNTIME_HOOKS_PROVIDER__, previous);
    const connected = runtime.connect();
    const rejected = assert.rejects(connected, /stopped|closed/);
    await requestArrived;
    runtime.close();
    await rejected;
    assert.equal(runtime.connected, false);
  } finally {
    runtime.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
});
