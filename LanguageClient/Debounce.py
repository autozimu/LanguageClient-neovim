import time

# <https://gist.github.com/esromneb/8eac6bf5bdfef58304cb>

class Debounce(object):
    def __init__(self, period):
        self.period = period  # never call the wrapped function more often than this (in seconds)
        self.last = None  # the last time it was called

    def reset(self):
        self.last = None

    def __call__(self, f):
        def wrapped(*args, **kwargs):
            now = time.time()
            willcall = False
            if self.last is not None:
                # amount of time since last call
                delta = now - self.last
                if delta >= self.period:
                    willcall = True
                else:
                    willcall = False
            else:
                willcall = True  # function has never been called before

            if willcall:
                # set these first incase we throw an exception
                self.last = now  # don't use time.time()
                f(*args, **kwargs)  # call wrapped function
        return wrapped
