"""Actual exec + inherited descriptors, with no running WeaverTools service."""
import fcntl
import json
import os
from pathlib import Path
import socket
import sys
import pytest
from python_spu.client import reap,seal_inheritance
from python_spu.transport import Channel
from python_spu.wire import dump

@pytest.fixture
def child(tmp_path):
    pairs=[socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET) for _ in range(2)]
    log=(tmp_path/'stderr.txt').open('wb')
    copies=[fcntl.fcntl(pair[1].fileno(),fcntl.F_DUPFD_CLOEXEC,10) for pair in pairs]
    actions=[(os.POSIX_SPAWN_DUP2,copies[0],3),(os.POSIX_SPAWN_DUP2,copies[1],4),
             (os.POSIX_SPAWN_DUP2,log.fileno(),2)]
    actions += [(os.POSIX_SPAWN_CLOSE,fd) for fd in copies]
    seal_inheritance()
    pid=os.posix_spawn(sys.executable,[sys.executable,'-m','python_spu.server','--cpu-experiment'],
                       dict(os.environ),file_actions=actions)
    for fd in copies: os.close(fd)
    for _,sock in pairs: sock.close()
    lifecycle=Channel(pairs[0][0]); decode=Channel(pairs[1][0],True)
    for channel in (lifecycle,decode): channel.sock.settimeout(20)
    yield lifecycle,decode,pid,tmp_path/'stderr.txt'
    for channel in (lifecycle,decode): channel.sock.close()
    code=reap(pid)
    log.close()
    assert code==0,(tmp_path/'stderr.txt').read_text()

def lifecycle_ask(channel,body,ordinal=0):
    channel.send({'exchange':{'opener':'harness','ordinal':ordinal},'position':'open',
                  'payload':{'kind':'directive','body':body}})
    return channel.receive()

def test_exec_admit_generate_flush_release(child,tiny_model,instruction,oracle):
    life,decode,pid,log=child
    body=dump(instruction); body['decoder']['model-binding']['artifact']=str(tiny_model)
    assert lifecycle_ask(life,{'kind':'release'})['payload']=={'kind':'refusal','body':{'kind':'out_of_order'}}
    assert lifecycle_ask(life,{'kind':'admit','instruction':body},1)['payload']=={'kind':'answer','body':{'kind':'admitted'}}
    decode.send({'kind':'open','session':'s','messages':[{'role':'system','content':[{'type':'text','text':'be precise'}]}]})
    assert decode.receive()=={'kind':'opened'}
    decode.send({'kind':'append_and_generate','turn':'t','delta':[{'role':'user','content':[{'type':'text','text':'hello world'}]}]})
    frames=[]
    while True:
        frame=decode.receive(); frames.append(frame)
        assert 'ok' in oracle(op='round',type='TokenAnswer',body=frame)
        if frame['kind']=='generated': break
    assert frames[-1]['body']['resident']>0
    decode.send({'kind':'flush','keep':0})
    assert decode.receive()['kind']=='flushed'
    decode.send({'kind':'cancel','turn':'t'})
    assert decode.receive()=={'kind':'at_rest'}
    decode.sock.shutdown(socket.SHUT_RDWR)
    assert lifecycle_ask(life,{'kind':'release'},2)['payload']=={'kind':'answer','body':{'kind':'released'}}
    assert lifecycle_ask(life,{'kind':'release'},3)['payload']=={'kind':'refusal','body':{'kind':'out_of_order'}}

def test_failed_admission_spends_only_attempt(child,instruction):
    life,_,_,_=child
    body=dump(instruction)
    assert lifecycle_ask(life,{'kind':'admit','instruction':body})['payload']['body']['kind']=='artifact_unresolvable'
    assert lifecycle_ask(life,{'kind':'admit','instruction':body},1)['payload']['body']['kind']=='out_of_order'
