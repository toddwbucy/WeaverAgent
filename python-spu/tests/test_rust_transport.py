import fcntl
import os
from pathlib import Path
import socket
import pytest
from python_spu.client import reap
from python_spu.transport import Channel,PACKET

@pytest.mark.parametrize('size',[100,PACKET+100,1024*1024])
def test_python_to_rust_and_back(size):
    a,b=socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET)
    fd=fcntl.fcntl(b.fileno(),fcntl.F_DUPFD_CLOEXEC,10)
    exe=str(Path('oracle/target/debug/python-spu-oracle').resolve())
    pid=os.posix_spawn(exe,[exe,'--echo'],dict(os.environ),file_actions=[
        (os.POSIX_SPAWN_DUP2,fd,3),(os.POSIX_SPAWN_CLOSE,fd)])
    os.close(fd); b.close(); a.settimeout(10)
    try:
        channel=Channel(a,True); value={'kind':'test','text':'x'*size}
        channel.send(value)
        assert channel.receive()==value
    finally:
        a.close()
        # The echo loop observes EOF and exits, no inherited peer remaining, and is
        # waited on within a bound either way.
        code=reap(pid)
    assert code==0
