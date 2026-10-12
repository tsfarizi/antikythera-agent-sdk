# Declarative Tools (Sugar Layer)

Sugar layer class/function-based untuk registrasi tool tanpa boilerplate registry+gate. Mirror kontrak `server/registry.py` (shape golden 6-key, collision existing-first, sort deterministik, deepCopy) dan `server/gate.py` (register != allow strict).

## Ringkasan

Sugar hanya concern **definisi + handler binding**; nol global auto-registry, nol gate auto-allow. Translasi ke `UnionRegistry` + `PolicyGate` terjadi eksplisit via `register_tool(registry, gate, obj, allow=bool)`. Owner disimpan terpisah dari definition (definition = 6-key golden, owner = `server`|`client`|`mcp`).

## Contoh

### Python — class `BaseTool`

```python
from antikythera_agent.tools import BaseTool, register_tool
from antikythera_agent.server.registry import UnionRegistry
from antikythera_agent.server.gate import PolicyGate

class SearchTool(BaseTool):
    name = "search"
    description = "Search the index"
    title = "Search"
    input_schema = {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}
    owner = "server"
    def handle(self, inp):
        return {"results": []}

# is_tool(SearchTool) == True, extract_tool_entry(SearchTool) -> {definition, handler, owner}
registry = UnionRegistry()
gate = PolicyGate()
register_tool(registry, gate, SearchTool, allow=True)  # register + allowlist
```

### Python — decorator `@tool`

```python
from antikythera_agent.tools import tool

@tool("echo", "Echo input", owner="client", input_schema={"type": "object", "properties": {}, "required": []})
def echo(args):
    return {"echoed": args}
# echo.__tool_definition__(), echo.__tool_handler__(), echo.__tool_owner__
```

### JS — `defineTool`

```javascript
import { defineTool } from 'antikythera-agent/runtime';
import { createUnionRegistry } from 'antikythera-agent/runtime';
import { createPolicyGate } from 'antikythera-agent/runtime';

const search = defineTool({ name: "search", description: "Search the index" }, (args) => ({ results: [] }), "server");
// isTool(search) == true, extractToolEntry(search).definition == 6-key
```

## Tabel Kontrak CC-1..CC-4

| ID | Klausa | Validasi |
|----|--------|----------|
| CC-1 | `BaseTool` class sugar | `name`/`description` non-empty di `__init_subclass__` (ValueError), `owner` ∈ {server,client,mcp}, `title` str|None, `input_schema`/`output_schema` dict|None, `parameters` list|None, `__tool_definition__()` 6-key deepcopy + normalize, `__tool_handler__()` instansiasi per call |
| CC-2 | `@tool` decorator | `name`/`description` non-empty (ValueError), `owner` valid, marker `__antikythera_tool__` tanpa auto-registry, `__tool_definition__` deepcopy, `__tool_owner__` |
| CC-3 | Helpers `register_tool`/`is_tool`/`extract_tool_entry` | `is_tool` cek marker+callable, `extract` validasi keys ⊆ golden (ValueError jika extra), owner valid, deepCopy definition, `register_tool` dispatch ke `registry.register_*` sesuai owner, collision existing-first pesan `tool registry: name collision for tool 'X' (owners A, B)`, same-owner replace, `allow=True` → `gate.allow_tool(owner,name)` else tidak, `allow` harus bool, `gate` wajib jika allow=True |
| CC-4 | Integrasi bridge/agent/orchestrator | `AgentServer.register_tool(obj, allow)` + `register_tools(iterable, allow)` bulk loop, `Agent.register_tool`/`Orchestrator.register_tool` simpan pending `[(obj,allow)]` dan inject ephemeral `UnionRegistry+PolicyGate` pada `run`/`dispatch` berikutnya, validasi `is_tool` 즉시 TypeError, sort deterministik `definitions()` by name |

## File Baru

- `clients/python/antikythera_agent/tools/base.py` — `BaseTool`, `DEFAULT_INPUT_SCHEMA`
- `clients/python/antikythera_agent/tools/decorator.py` — `tool`
- `clients/python/antikythera_agent/tools/helpers.py` — `register_tool`, `is_tool`, `extract_tool_entry`
- `clients/python/antikythera_agent/tools/__init__.py` — re-export facade
- `clients/npm/antikythera-sdk/runtime/tools.js` — `defineTool`, `isTool`, `extractToolEntry`, `registerTool`
- `tests/python/test_tools_sugar.py` — falsifikasi CC-1..CC-4 (16 tests)
- `tests/node/tools.test.mjs` — parity JS (18 tests)

## Cara Pakai

```python
# Bridge server
from antikythera_agent.server import createAgentServer
server = createAgentServer()
server.register_tool(SearchTool, allow=True)
server.register_tools([SearchTool, EchoTool], allow=True)

# Agent (local loop)
from antikythera_agent import Agent
agent = Agent(provider="stub", model="x")
agent.register_tool(SearchTool, allow=True)
agent.run("hello")  # inject pending registry+gate

# Orchestrator
from antikythera_agent import Orchestrator
orch = Orchestrator()
orch.register_tool(SearchTool, allow=True)
orch.dispatch("task")
```

```javascript
import { defineTool, registerTool } from 'antikythera-agent/runtime';
import { normalizeLocalTools } from 'antikythera-agent/runtime'; // atau types.js

const t = defineTool({ name: "search", description: "d" }, handler, "server");
normalizeLocalTools([t]) // unwrap sugar -> [{definition, handler}]
createUnionRegistry({ localEntries: normalizeLocalTools([t]) })
```

## Amplop Validitas

- Shape definition **persis** 6-key golden `{name,title,description,parameters,input_schema,output_schema}`; `title`/`output_schema` null jika None, `parameters` [], `input_schema` DEFAULT object kosong.
- DeepCopy simpan+return: mutasi input dict maupun hasil `definitions()`/`__tool_definition__()` tidak bocor ke registry.
- Sort deterministik `definitions()` by `name` ascending.
- Destination `server`/`client`/`mcp` saja; cross-owner collision = ValueError eksplisit existing-first.

## Larangan

- **Nol global auto-registry on import**: import `antikythera_agent.tools` tidak mendaftarkan tool; registrasi hanya via `register_tool`.
- **Nol gate auto-allow**: `register_tool(..., allow=False)` default; tidak ada allow implisit.
- **Nol field ekstra**: key di luar golden → ValueError `tool definition contains invalid keys [...]`.

## Migrasi dari dict manual ke class

```python
# sebelum (dict manual)
registry.register_server({"name": "search", "description": "Search", "input_schema": {...}}, handler=fn)
gate.allow_tool("server", "search")

# sesudah (sugar)
class SearchTool(BaseTool):
    name = "search"; description = "Search"; input_schema = {...}
    def handle(self, inp): return fn(inp)
register_tool(registry, gate, SearchTool, allow=True)
# atau decorator
@tool("search", "Search", input_schema={...})
def search(inp): return fn(inp)
register_tool(registry, gate, search, allow=True)
```

Jalankan test:

```bash
pytest tests/python/test_tools_sugar.py -v
node --test tests/node/tools.test.mjs
```
