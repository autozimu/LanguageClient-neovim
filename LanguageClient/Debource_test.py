from . Debounce import Debounce

class A:
    def __init__(self):
        self.n = 0

    @Debounce(1.0)
    def increment(self):
        self.n += 1

def test_Debounce():
    a = A()
    a.increment()
    a.increment()
    assert a.n == 1
