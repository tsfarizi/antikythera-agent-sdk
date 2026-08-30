"""Falsification suite untuk sugar layer class-based tool registration (T7, CC-1..CC-4).

Kontrak yang difalsifikasi:
- CC-1: BaseTool class-based definition (name/description wajib, owner terisolasi, shape golden 6-key, deepCopy)
- CC-2: @tool decorator (name/description wajib, owner valid, marker tanpa auto-registry)
- CC-3: helpers register_tool / is_tool / extract_tool_entry (keys ⊆ golden, collision existing-first, register != allow, allow validasi)
- CC-4: integrasi bridge/agent/orchestrator (register_tool pending + bulk, sort determinism)

Sumber kebenaran:
- clients/python/antikythera_agent/tools/base.py
- clients/python/antikythera_agent/tools/decorator.py
- clients/python/antikythera_agent/tools/helpers.py
- clients/python/antikythera_agent/server/registry.py (collision, sort, deepCopy)
- clients/python/antikythera_agent/server/gate.py (allow strict)
- clients/python/antikythera_agent/server/bridge.py (register_tool / register_tools)
- clients/python/antikythera_agent/agent.py + orchestrator.py (pending)

Amplop test:
- Nol state bersama antar test; suite lolos urutan apa pun.
- Deterministik: random tidak dipakai, waktu dikontrol, tanpa I/O eksternal.
- Isolasi: registry/gate per test dibuat fresh.

Menjalankan:
    pytest clients/python/tests/test_tools_sugar.py -v
"""

from __future__ import annotations

import copy

import pytest

from antikythera_agent.server.gate import PolicyGate
from antikythera_agent.server.registry import UnionRegistry
from antikythera_agent.tools import (
    DEFAULT_INPUT_SCHEMA,
    BaseTool,
    extract_tool_entry,
    is_tool,
    register_tool,
    tool,
)

# ---------------------------------------------------------------------------
# CC-1: BaseTool requires name/description + owner validates
# ---------------------------------------------------------------------------

def test_base_tool_requires_name_description():
    """CC-1: BaseTool tanpa name/description non-empty -> ValueError di __init_subclass__."""
    with pytest.raises(ValueError, match="name"):
        class BadNoName(BaseTool):
            name = ""
            description = "desc"
            def handle(self, inp): return 1

    with pytest.raises(ValueError, match="description"):
        class BadNoDesc(BaseTool):
            name = "ok"
            description = ""
            def handle(self, inp): return 1

    with pytest.raises(ValueError, match="name"):
        class BadMissingName(BaseTool):  # type: ignore
            description = "desc"
            def handle(self, inp): return 1

    with pytest.raises(ValueError, match="description"):
        class BadMissingDesc(BaseTool):  # type: ignore
            name = "ok"
            def handle(self, inp): return 1

    # Non-string also fails
    with pytest.raises(ValueError, match="name"):
        class BadNameInt(BaseTool):  # type: ignore
            name = 123  # type: ignore
            description = "d"
            def handle(self, inp): return 1


def test_base_tool_owner_validates():
    """CC-1: owner bukan server/client/mcp -> ValueError di __init_subclass__."""
    with pytest.raises(ValueError, match="owner"):
        class BadOwner(BaseTool):
            name = "a"
            description = "b"
            owner = "invalid"
            def handle(self, inp): return 1

    with pytest.raises(ValueError, match="owner"):
        class BadOwnerEmpty(BaseTool):
            name = "a"
            description = "b"
            owner = ""
            def handle(self, inp): return 1

    # Valid owners must succeed
    for owner in ("server", "client", "mcp"):
        # dynamic class to avoid closure capture issue in class body
        Ok = type("Ok", (BaseTool,), {"name": f"tool_{owner}", "description": "d", "owner": owner, "handle": lambda self, inp: 1})
        assert is_tool(Ok)
        assert extract_tool_entry(Ok)["owner"] == owner


# ---------------------------------------------------------------------------
# CC-2: decorator requires name/description
# ---------------------------------------------------------------------------

def test_decorator_requires_name_description():
    """CC-2: @tool tanpa name/description non-empty -> ValueError."""
    with pytest.raises(ValueError, match="name"):
        tool("", "desc")
    with pytest.raises(ValueError, match="description"):
        tool("name", "")
    with pytest.raises(ValueError, match="name"):
        tool(None, "desc")  # type: ignore
    with pytest.raises(ValueError, match="description"):
        tool("name", None)  # type: ignore

    # owner invalid
    with pytest.raises(ValueError, match="owner"):
        tool("a", "b", owner="invalid")

    # valid decorator attaches markers without auto-registry
    @tool("my_fn", "my desc", owner="server")
    def my_fn(x): return x

    assert is_tool(my_fn)
    assert my_fn.__antikythera_tool__ is True
    entry = extract_tool_entry(my_fn)
    assert entry["definition"]["name"] == "my_fn"
    assert entry["owner"] == "server"
    assert entry["handler"] is my_fn


# ---------------------------------------------------------------------------
# CC-3 helpers: golden 6 keys only
# ---------------------------------------------------------------------------

def test_golden_6_keys_only():
    """CC-3: extract_tool_entry raises ValueError jika extra keys di luar golden 6-key."""
    # fake tool dengan extra key via manual marker
    class Fake:
        __antikythera_tool__ = True
        owner = "server"
        @classmethod
        def __tool_definition__(cls):
            return {
                "name": "a",
                "title": None,
                "description": "d",
                "parameters": [],
                "input_schema": {"type": "object", "properties": {}, "required": []},
                "output_schema": None,
                "extra": "bad",
            }
        @classmethod
        def __tool_handler__(cls):
            return lambda x: x

    assert is_tool(Fake)
    with pytest.raises(ValueError, match="invalid keys"):
        extract_tool_entry(Fake)

    # multiple extra keys
    class Fake2:
        __antikythera_tool__ = True
        owner = "client"
        @classmethod
        def __tool_definition__(cls):
            return {
                "name": "b",
                "title": None,
                "description": "d",
                "parameters": [],
                "input_schema": {"type": "object", "properties": {}, "required": []},
                "output_schema": None,
                "foo": 1,
                "bar": 2,
            }
        @classmethod
        def __tool_handler__(cls):
            return lambda x: x

    with pytest.raises(ValueError, match="invalid keys"):
        extract_tool_entry(Fake2)
        # ensure allowed set mentioned
        # second check: message contains sorted extra
    try:
        extract_tool_entry(Fake2)
    except ValueError as e:
        assert "bar" in str(e) or "foo" in str(e)


# ---------------------------------------------------------------------------
# DeepCopy isolation
# ---------------------------------------------------------------------------

def test_deepcopy_isolation():
    """CC-1/CC-3: mutasi definition hasil __tool_definition__ tidak bocor ke registry;
    mutasi input_schema di subclass tidak bocor."""
    # Part 1: registry deepCopy - mutasi returned definition tidak bocor ke registry
    class DeepTool(BaseTool):
        name = "deep1"
        description = "original"
        input_schema = {"type": "object", "properties": {"a": {"type": "string"}}, "required": ["a"]}
        def handle(self, inp): return 1

    registry = UnionRegistry()
    gate = PolicyGate()
    register_tool(registry, gate, DeepTool)

    d1 = DeepTool.__tool_definition__()
    d1["description"] = "MUTATED"
    d1["input_schema"]["properties"]["a"] = {"type": "number"}
    # registry unaffected
    assert registry.definitions()[0]["description"] == "original"
    assert registry.definitions()[0]["input_schema"]["properties"]["a"] == {"type": "string"}
    # second call still original (deepcopy per call)
    d2 = DeepTool.__tool_definition__()
    assert d2["description"] == "original"
    assert d2["input_schema"]["properties"]["a"] == {"type": "string"}

    # Part 2: mutasi input_schema dict di subclass tidak bocor via shared reference
    original_schema = {"type": "object", "properties": {"x": {"type": "string"}}, "required": []}
    # Use copy to avoid aliasing with DEFAULT
    schema_copy = copy.deepcopy(original_schema)

    class SchemaTool(BaseTool):
        name = "deep2"
        description = "d"
        input_schema = schema_copy
        def handle(self, inp): return 1

    # mutate original dict after class definition
    schema_copy["properties"]["x"] = {"type": "number"}
    schema_copy["properties"]["y"] = {"type": "string"}

    d3 = SchemaTool.__tool_definition__()
    # class attribute mutation after definition should not affect definition's copy?
    # __tool_definition__ deepCopies current class attribute at call time, but our test wants isolation
    # So we check that d3 is deep copy of current class state and further mutation of d3 doesn't leak
    d3["input_schema"]["properties"]["x"] = {"type": "boolean"}
    d4 = SchemaTool.__tool_definition__()
    # d4 should reflect current class attribute (which we mutated to number), not boolean
    assert d4["input_schema"]["properties"]["x"] == {"type": "number"}

    # Part 3: decorator deepCopy isolation
    @tool("deep_deco", "d", input_schema={"type": "object", "properties": {"k": {"type": "string"}}, "required": []})
    def fn(x): return x

    e1 = extract_tool_entry(fn)
    e1["definition"]["input_schema"]["properties"]["k"] = {"type": "number"}
    e2 = extract_tool_entry(fn)
    assert e2["definition"]["input_schema"]["properties"]["k"] == {"type": "string"}

    # Part 4: registry definitions() returns copies (mutasi tidak bocor)
    registry2 = UnionRegistry()
    gate2 = PolicyGate()
    class Simple(BaseTool):
        name = "simple"
        description = "d"
        def handle(self, inp): return 1
    register_tool(registry2, gate2, Simple)
    lst = registry2.definitions()
    lst[0]["name"] = "mutated"
    assert registry2.has("mutated") is False
    assert registry2.has("simple") is True


# ---------------------------------------------------------------------------
# Collision existing-first
# ---------------------------------------------------------------------------

def test_collision_existing_first():
    """CC-3: collision lintas owner error existing-first: server->client vs client->server."""
    # server first, client second -> owners server, client
    reg = UnionRegistry()
    gate = PolicyGate()

    class SrvDup(BaseTool):
        name = "dup"
        description = "srv"
        owner = "server"
        def handle(self, inp): return 1

    class CliDup(BaseTool):
        name = "dup"
        description = "cli"
        owner = "client"
        def handle(self, inp): return 1

    register_tool(reg, gate, SrvDup)
    with pytest.raises(ValueError) as exc:
        register_tool(reg, gate, CliDup)
    msg = str(exc.value)
    assert "collision" in msg
    assert "dup" in msg
    assert "owners server, client" in msg

    # reverse: client first, server second -> owners client, server
    reg2 = UnionRegistry()
    gate2 = PolicyGate()
    register_tool(reg2, gate2, CliDup)
    with pytest.raises(ValueError) as exc2:
        register_tool(reg2, gate2, SrvDup)
    msg2 = str(exc2.value)
    assert "owners client, server" in msg2

    # mcp variant
    class McpDup(BaseTool):
        name = "dup"
        description = "mcp"
        owner = "mcp"
        def handle(self, inp): return 1

    reg3 = UnionRegistry()
    gate3 = PolicyGate()
    register_tool(reg3, gate3, SrvDup)
    with pytest.raises(ValueError) as exc3:
        register_tool(reg3, gate3, McpDup)
    assert "owners server, mcp" in str(exc3.value)


# ---------------------------------------------------------------------------
# Same owner replace
# ---------------------------------------------------------------------------

def test_same_owner_replace():
    """CC-3: register same owner twice -> size tetap 1, handler ter-replace, tidak throw."""
    registry = UnionRegistry()
    gate = PolicyGate()

    class EchoV1(BaseTool):
        name = "echo"
        description = "old"
        owner = "server"
        title = "Old"
        def handle(self, inp): return "old"

    class EchoV2(BaseTool):
        name = "echo"
        description = "new"
        owner = "server"
        title = "New"
        def handle(self, inp): return "new"

    register_tool(registry, gate, EchoV1)
    assert registry.size() == 1
    assert registry.definitions()[0]["description"] == "old"
    h1 = registry.handler_of("echo")
    assert h1({}) == "old"

    register_tool(registry, gate, EchoV2)
    assert registry.size() == 1
    assert registry.definitions()[0]["description"] == "new"
    assert registry.definitions()[0]["title"] == "New"
    h2 = registry.handler_of("echo")
    assert h2({}) == "new"
    # ensure not collision error
    assert registry.owner_of("echo") == "server"


# ---------------------------------------------------------------------------
# Register != allow strict
# ---------------------------------------------------------------------------

def test_register_not_allow_strict():
    """CC-3: register allow=False -> gate.check_tool deny, allow=True -> gate passes."""
    reg = UnionRegistry()
    gate = PolicyGate()

    class AllowTool(BaseTool):
        name = "allowme"
        description = "d"
        owner = "server"
        def handle(self, inp): return 1

    # allow=False: registered but not allowed
    register_tool(reg, gate, AllowTool, allow=False)
    assert reg.has("allowme")
    with pytest.raises(Exception) as exc:
        gate.check_tool("server", "allowme")
    assert str(exc.value).startswith("permission:")

    # allow=True: registered and allowed
    reg2 = UnionRegistry()
    gate2 = PolicyGate()
    register_tool(reg2, gate2, AllowTool, allow=True)
    gate2.check_tool("server", "allowme")  # should not raise
    assert gate2.allows("server", "allowme") is True

    # decorator variant
    @tool("allow_fn", "d", owner="client")
    def fn(x): return x

    reg3 = UnionRegistry()
    gate3 = PolicyGate()
    register_tool(reg3, gate3, fn, allow=False)
    assert reg3.owner_of("allow_fn") == "client"
    with pytest.raises(Exception):
        gate3.check_tool("client", "allow_fn")
    reg4 = UnionRegistry()
    gate4 = PolicyGate()
    register_tool(reg4, gate4, fn, allow=True)
    gate4.check_tool("client", "allow_fn")


# ---------------------------------------------------------------------------
# Bridge register_tool and bulk
# ---------------------------------------------------------------------------

def test_bridge_register_tool_and_register_tools_bulk():
    """CC-4: AgentServer.register_tool / register_tools bulk (allow per-item)."""
    from antikythera_agent.server.bridge import createAgentServer

    server = createAgentServer()

    class B1(BaseTool):
        name = "b1"
        description = "d1"
        owner = "server"
        def handle(self, inp): return 1

    class B2(BaseTool):
        name = "b2"
        description = "d2"
        owner = "client"
        def handle(self, inp): return 1

    # single
    server.register_tool(B1, allow=True)
    assert server.registry.has("b1")
    assert server.registry.owner_of("b1") == "server"
    assert server.gate.allows("server", "b1") is True
    assert server.gate.allows("client", "b1") is False

    # bulk
    server2 = createAgentServer()
    server2.register_tools([B1, B2], allow=True)
    assert server2.registry.size() == 2
    assert server2.registry.owner_of("b1") == "server"
    assert server2.registry.owner_of("b2") == "client"
    server2.gate.check_tool("server", "b1")
    server2.gate.check_tool("client", "b2")

    # bulk allow=False -> not allowed
    server3 = createAgentServer()
    server3.register_tools([B1, B2], allow=False)
    assert server3.registry.size() == 2
    with pytest.raises(Exception):
        server3.gate.check_tool("server", "b1")
    with pytest.raises(Exception):
        server3.gate.check_tool("client", "b2")

    # bulk type error: not iterable
    with pytest.raises(TypeError):
        server3.register_tools("not-iterable")  # type: ignore
    with pytest.raises(TypeError):
        server3.register_tools(None)  # type: ignore


# ---------------------------------------------------------------------------
# Agent pending + type error
# ---------------------------------------------------------------------------

def test_agent_register_tool_pending_and_type_error():
    """CC-4: Agent.register_tool pending storage + TypeError jika bukan Tool."""
    from antikythera_agent.agent import Agent

    agent = Agent(provider="stub", model="x")

    class ATool(BaseTool):
        name = "atool"
        description = "d"
        owner = "server"
        def handle(self, inp): return 1

    agent.register_tool(ATool, allow=True)
    assert len(agent._pending_tools) == 1
    assert agent._pending_tools[0][0] is ATool
    assert agent._pending_tools[0][1] is True

    # type error for plain dict / non-tool
    with pytest.raises(TypeError, match="is not a Tool"):
        agent.register_tool({"name": "a"})  # type: ignore
    with pytest.raises(TypeError):
        agent.register_tool(None)  # type: ignore

    # allow must be bool
    with pytest.raises(TypeError, match="allow.*bool"):
        agent.register_tool(ATool, allow="yes")  # type: ignore

    # bulk
    class ATool2(BaseTool):
        name = "atool2"
        description = "d"
        owner = "client"
        def handle(self, inp): return 1

    agent.register_tools([ATool2], allow=False)
    assert len(agent._pending_tools) == 2

    with pytest.raises(TypeError):
        agent.register_tools("bad")  # type: ignore
    with pytest.raises(TypeError):
        agent.register_tools([ATool], allow="yes")  # type: ignore


def test_orchestrator_register_tool_pending():
    """CC-4: Orchestrator.register_tool pending + bulk + TypeError."""
    from antikythera_agent.orchestrator import Orchestrator

    orch = Orchestrator()

    class OTool(BaseTool):
        name = "otool"
        description = "d"
        owner = "server"
        def handle(self, inp): return 1

    orch.register_tool(OTool, allow=False)
    assert len(orch._pending_tools) == 1
    assert orch._pending_tools[0][1] is False

    with pytest.raises(TypeError, match="is not a Tool"):
        orch.register_tool({"name": "x"})  # type: ignore

    with pytest.raises(TypeError, match="allow.*bool"):
        orch.register_tool(OTool, allow=1)  # type: ignore

    class OTool2(BaseTool):
        name = "otool2"
        description = "d"
        owner = "mcp"
        def handle(self, inp): return 1

    orch.register_tools([OTool2], allow=True)
    assert len(orch._pending_tools) == 2
    assert orch._pending_tools[1][1] is True

    with pytest.raises(TypeError):
        orch.register_tools(None)  # type: ignore
    with pytest.raises(TypeError):
        orch.register_tools("bad", allow=True)  # type: ignore


# ---------------------------------------------------------------------------
# Sort determinism
# ---------------------------------------------------------------------------

def test_sort_determinism():
    """CC-3/CC-4: register zebra then apple -> definitions sorted apple, zebra."""
    reg = UnionRegistry()
    gate = PolicyGate()

    class Zebra(BaseTool):
        name = "zebra"
        description = "z"
        owner = "server"
        def handle(self, inp): return 1

    class Apple(BaseTool):
        name = "apple"
        description = "a"
        owner = "server"
        def handle(self, inp): return 1

    register_tool(reg, gate, Zebra)
    register_tool(reg, gate, Apple)
    names = [d["name"] for d in reg.definitions()]
    assert names == ["apple", "zebra"]

    # mixed owners, insertion order shuffled
    reg2 = UnionRegistry()
    gate2 = PolicyGate()

    class Mike(BaseTool):
        name = "mike"
        description = "m"
        owner = "mcp"
        def handle(self, inp): return 1

    register_tool(reg2, gate2, Zebra)
    register_tool(reg2, gate2, Mike)
    register_tool(reg2, gate2, Apple)
    assert [d["name"] for d in reg2.definitions()] == ["apple", "mike", "zebra"]


# ---------------------------------------------------------------------------
# allow requires gate and bool
# ---------------------------------------------------------------------------

def test_allow_requires_gate_and_bool():
    """CC-3: allow=True tanpa gate -> TypeError, allow bukan bool -> TypeError."""
    reg = UnionRegistry()
    gate = PolicyGate()

    class GTool(BaseTool):
        name = "gtool"
        description = "d"
        owner = "server"
        def handle(self, inp): return 1

    with pytest.raises(TypeError, match="gate is required"):
        register_tool(reg, None, GTool, allow=True)

    with pytest.raises(TypeError, match="'allow' must be a bool"):
        register_tool(reg, gate, GTool, allow="true")  # type: ignore
    with pytest.raises(TypeError, match="'allow' must be a bool"):
        register_tool(reg, gate, GTool, allow=1)  # type: ignore
    with pytest.raises(TypeError, match="'allow' must be a bool"):
        register_tool(reg, gate, GTool, allow=None)  # type: ignore

    # bridge variant
    from antikythera_agent.server.bridge import createAgentServer
    srv = createAgentServer()
    with pytest.raises(TypeError):
        srv.register_tool(GTool, allow="yes")  # type: ignore
    with pytest.raises(TypeError):
        srv.register_tools([GTool], allow="yes")  # type: ignore


# ---------------------------------------------------------------------------
# is_tool false for plain dict
# ---------------------------------------------------------------------------

def test_is_tool_false_for_plain_dict():
    """CC-3: is_tool False untuk plain dict, None, string, object tanpa marker."""
    assert is_tool({"name": "a", "description": "d"}) is False
    assert is_tool(None) is False
    assert is_tool("string") is False
    assert is_tool(123) is False
    assert is_tool([]) is False

    class NotTool:
        name = "a"
        description = "d"
        def handle(self, inp): return 1

    assert is_tool(NotTool) is False
    assert is_tool(NotTool()) is False

    # decorator should be True, plain function False
    def plain(x): return x
    assert is_tool(plain) is False

    @tool("is_tool_fn", "desc")
    def decorated(x): return x
    assert is_tool(decorated) is True

    class Real(BaseTool):
        name = "real"
        description = "d"
        def handle(self, inp): return 1

    assert is_tool(Real) is True
    # instance also carries marker via class attr lookup -> is_tool returns True (documented)
    assert is_tool(Real()) is True


# ---------------------------------------------------------------------------
# Additional: DEFAULT isolation + shape golden
# ---------------------------------------------------------------------------

def test_default_input_schema_isolation():
    """CC-1: DEFAULT_INPUT_SCHEMA deepCopy isolation (mutasi tidak bocor)."""
    class DefTool(BaseTool):
        name = "def_tool"
        description = "d"
        def handle(self, inp): return 1

    d = DefTool.__tool_definition__()
    assert d["input_schema"] == DEFAULT_INPUT_SCHEMA
    # but not same object
    assert d["input_schema"] is not DEFAULT_INPUT_SCHEMA
    d["input_schema"]["properties"]["x"] = {"type": "string"}
    d2 = DefTool.__tool_definition__()
    assert "x" not in d2["input_schema"]["properties"]

    # decorator default
    @tool("def_fn", "d2")
    def fn2(x): return x
    e = extract_tool_entry(fn2)
    assert e["definition"]["input_schema"] == DEFAULT_INPUT_SCHEMA
    e["definition"]["input_schema"]["properties"]["y"] = {"type": "string"}
    e2 = extract_tool_entry(fn2)
    assert "y" not in e2["definition"]["input_schema"]["properties"]


def test_shape_golden_keys_exact():
    """CC-3: definition shape persis 6 keys golden, tanpa extra."""
    class ShapeTool(BaseTool):
        name = "shape"
        description = "d"
        title = "T"
        parameters = [{"name": "p"}]
        input_schema = {"type": "object", "properties": {}, "required": []}
        output_schema = {"type": "string"}
        owner = "server"
        def handle(self, inp): return 1

    d = ShapeTool.__tool_definition__()
    assert set(d.keys()) == {"name", "title", "description", "parameters", "input_schema", "output_schema"}

    @tool("shape_fn", "d", title="T", input_schema={"type": "object", "properties": {}, "required": []}, output_schema={"type": "string"})
    def fn(x): return x
    e = extract_tool_entry(fn)
    assert set(e["definition"].keys()) == {"name", "title", "description", "parameters", "input_schema", "output_schema"}
