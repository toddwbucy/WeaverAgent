import socket
import threading
import pytest
from python_spu.transport import Channel,ChannelFault,Closed,PACKET,MESSAGE

def pair():
    a,b=socket.socketpair(socket.AF_UNIX,socket.SOCK_SEQPACKET)
    a.settimeout(3); b.settimeout(3)
    return Channel(a,True),Channel(b,True)

@pytest.mark.parametrize('length',[1,PACKET-100,PACKET+100,3*PACKET])
def test_segmented_roundtrip(length):
    a,b=pair(); body={'kind':'test','payload':'β'*length}
    errors=[]
    def send():
        try: a.send(body)
        except Exception as e: errors.append(e)
    thread=threading.Thread(target=send); thread.start()
    try: assert b.receive()==body
    finally: thread.join(5); a.sock.close(); b.sock.close()
    assert not errors and not thread.is_alive()

@pytest.mark.parametrize('body',[
 {'segments':2,'bytes':100}, {'segments':3,'bytes':PACKET+1},
 {'segments':2,'bytes':PACKET+1,'extra':1},
 {'segments':True,'bytes':PACKET+1}, {'segments':129,'bytes':MESSAGE+1},{}])
def test_malformed_preamble_faults_without_waiting_for_slices(body):
    a,b=pair()
    try:
        a.send(body)
        with pytest.raises(ChannelFault): b.receive()
    finally: a.sock.close(); b.sock.close()

def test_truncation_and_eof():
    a,b=pair()
    a.sock.send(b'x'*(PACKET+1))
    with pytest.raises(ChannelFault,match='truncated'): b.receive()
    a.sock.close()
    with pytest.raises(Closed): b.receive()
    b.sock.close()

def test_partial_series_wrong_length():
    a,b=pair()
    try:
        a.send({'segments':2,'bytes':PACKET+1})
        a.send_packet(b'a'); a.send_packet(b'b')
        with pytest.raises(ChannelFault,match='underflow'): b.receive()
    finally: a.sock.close(); b.sock.close()

def test_send_bound_and_nonblocking():
    a,b=pair()
    try:
        # A timeout-mode socket overrides MSG_DONTWAIT with a readiness wait.
        b.sock.settimeout(None)
        assert b.receive(nonblocking=True) is None
        with pytest.raises(ChannelFault): a.send_bytes(b'x'*(MESSAGE+1))
    finally: a.sock.close(); b.sock.close()
