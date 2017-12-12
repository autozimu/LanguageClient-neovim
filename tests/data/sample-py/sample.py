import sys


def greet(name: str) -> str:
    """
    Greetings!
    """
    return "Yo, " + name


greet("there")

sys.stdin


# Multiple definitions
class Test1:
    def foo(self, x):
        print("Test1 %d" % x)


class Test2:
    def foo(self, x):
        print("Test2 %d" % x)


def bar(some_test_object):
    some_test_object.foo(42)


bar(Test1())
bar(Test2())
