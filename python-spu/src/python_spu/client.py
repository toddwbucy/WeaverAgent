"""Local test driver; starts only its own child and never addresses a server."""
import fcntl
import os
import signal
import socket
import sys
import time
from .transport import Channel

def seal_inheritance():
    """Marks every descriptor above 2 close-on-exec, so a child spawned next inherits
    its channel ends, dup2'd by the spawn's own actions, and nothing else. A descriptor
    held without close-on-exec, which python-build-standalone's ctypes leaves on the
    interpreter's own executable at import, would otherwise reach the child too."""
    for name in os.listdir('/proc/self/fd'):
        held=int(name)
        if held>2:
            try:
                if os.get_inheritable(held): os.set_inheritable(held,False)
            except OSError: pass

def reap(pid,timeout=10):
    """Waits for a child at most `timeout` seconds, kills it where it has not exited by
    then, and answers its exit code. Every wait on a child goes through here, so a
    child that never exits costs the bound and never the caller's whole run."""
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        done,status=os.waitpid(pid,os.WNOHANG)
        if done: return os.waitstatus_to_exitcode(status)
        time.sleep(.02)
    os.kill(pid,signal.SIGKILL); _,status=os.waitpid(pid,0)
    return os.waitstatus_to_exitcode(status)

class LocalProcess:
    def __init__(self,stderr,*,module='python_spu.server',arguments=('--cpu-experiment',),channels=2,command=None):
        pairs=[socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET) for _ in range(channels)]
        self.log=open(stderr,'wb')
        copies=[fcntl.fcntl(pair[1].fileno(),fcntl.F_DUPFD_CLOEXEC,10) for pair in pairs]
        log_copy=fcntl.fcntl(self.log.fileno(),fcntl.F_DUPFD_CLOEXEC,10)
        seal_inheritance()
        actions=[(os.POSIX_SPAWN_DUP2,fd,3+i) for i,fd in enumerate(copies)]
        actions.append((os.POSIX_SPAWN_DUP2,log_copy,2))
        actions.extend((os.POSIX_SPAWN_CLOSE,fd) for fd in copies+[log_copy])
        try:
            argv=list(command) if command else [sys.executable,'-m',module]
            self.pid=os.posix_spawn(argv[0],[*argv,*arguments],
                dict(os.environ),file_actions=actions)
        finally:
            for fd in copies+[log_copy]: os.close(fd)
            for _,sock in pairs: sock.close()
        self.channels=[Channel(pair[0],channels==2 and i==1) for i,pair in enumerate(pairs)]
        for channel in self.channels: channel.sock.settimeout(120)
        self.ordinal=0
    def ask(self,body):
        life=self.channels[0]
        life.send({'exchange':{'opener':'harness','ordinal':self.ordinal},'position':'open',
                   'payload':{'kind':'directive','body':body}})
        self.ordinal+=1
        return life.receive()
    def close(self,timeout=10,keep_open=False):
        """Closes the channels, waits for the child through `reap`, and answers its
        exit code. Where `keep_open`, the channels stay open until the child is
        reaped, for a child meant to exit on its own that would read a closed channel
        as its peer gone, and close once it has."""
        if not keep_open:
            for channel in self.channels: channel.sock.close()
        try:
            return reap(self.pid,timeout)
        finally:
            for channel in self.channels: channel.sock.close()
            self.log.close()
