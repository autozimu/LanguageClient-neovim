from .state import state, set_state


def test_set_state():
    set_state(["signs"], [1, 2, 3])
    assert state["signs"] == [1, 2, 3]
    set_state(["signs"], [])
    assert state["signs"] == []
    set_state(["a", "b", "c"], [1])
    assert state["a"]["b"]["c"] == [1]
