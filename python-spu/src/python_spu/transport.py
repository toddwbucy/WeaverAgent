"""Inherited SEQPACKET channels. Lifecycle never uses decode segmentation."""
import ctypes
import json
import os
import socket
from .wire import SegmentPreamble

PACKET = 64 * 1024
MESSAGE = 8 * 1024 * 1024
class ChannelFault(Exception): pass
class Closed(ChannelFault): pass

def encode(value):
    return json.dumps(value,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode()
def decode(body):
    def invalid(value): raise ValueError(f'nonfinite JSON: {value}')
    try: return json.loads(body,parse_constant=invalid)
    except (ValueError,UnicodeError) as e: raise ChannelFault('undecodable') from e

class Channel:
    def __init__(self,sock,segmented=False):
        self.sock=sock
        self.segmented=segmented
    def packet(self,flags=0):
        body,_,status,_=self.sock.recvmsg(PACKET,0,flags)
        if status & socket.MSG_TRUNC: raise ChannelFault('truncated packet')
        if not body: raise Closed('peer closed')
        return body
    def send_packet(self,body):
        if len(body)>PACKET: raise ChannelFault('truncated packet')
        if self.sock.send(body)!=len(body): raise ChannelFault('short send')
    def send(self,value):
        self.send_bytes(encode(value))
    def send_bytes(self,body):
        limit=MESSAGE if self.segmented else PACKET
        if len(body)>limit: raise ChannelFault('message exceeds bound')
        if len(body)>PACKET:
            self.send_packet(encode({'segments':(len(body)+PACKET-1)//PACKET,'bytes':len(body)}))
        for start in range(0,len(body),PACKET): self.send_packet(body[start:start+PACKET])
    def receive(self,nonblocking=False):
        try: first=self.packet(socket.MSG_DONTWAIT if nonblocking else 0)
        except BlockingIOError: return None
        value=decode(first)
        if self.segmented and isinstance(value,dict) and 'kind' not in value:
            try: pre=SegmentPreamble.model_validate(value)
            except ValueError as e: raise ChannelFault('bad preamble') from e
            if not PACKET<pre.bytes<=MESSAGE or pre.segments!=(pre.bytes+PACKET-1)//PACKET:
                raise ChannelFault('bad preamble bounds')
            chunks=[]
            size=0
            for _ in range(pre.segments):
                part=self.packet()
                size+=len(part)
                if size>pre.bytes: raise ChannelFault('segment overflow')
                chunks.append(part)
            if size!=pre.bytes: raise ChannelFault('segment underflow')
            value=decode(b''.join(chunks))
        return value

def adopt_channels(expected):
    # The count taken when the package was first imported, before any module opened
    # a descriptor of its own, so it holds what the process inherited and nothing else.
    from . import INHERITED
    held=list(INHERITED)
    if sorted(held)!=list(expected): raise ChannelFault(f'unexpected inherited descriptors: {held}')
    for fd in held: os.set_inheritable(fd,False)
    libc=ctypes.CDLL(None,use_errno=True)
    if libc.prctl(4,0,0,0,0)!=0: raise ChannelFault('cannot clear dumpability')
    sockets=[socket.socket(fileno=fd) for fd in expected]
    for sock in sockets:
        if sock.family!=socket.AF_UNIX or sock.getsockopt(socket.SOL_SOCKET,socket.SO_TYPE)!=socket.SOCK_SEQPACKET:
            raise ChannelFault('wrong channel type')
    return [Channel(sock,len(expected)==2 and i==1) for i,sock in enumerate(sockets)]

def adopt():
    return adopt_channels((3,4))
