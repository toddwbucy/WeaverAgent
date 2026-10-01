"""Local Qwen2, explicit placement, cached forwards and full rollback rebuild."""
import gc
import math
import os
import stat
from blake3 import blake3
from pathlib import Path

# The dtype the first version loads at, per python-spu-Spec sections 4 and 9: named
# once, used by the load and read by the smoke's report, so the two cannot disagree.
DTYPE='bfloat16'

# The headroom the Rust SPU compiles in, `HEADROOM_BYTES` in weaver-spu's main.rs, which
# stands wherever the worker's vector states none.
HEADROOM_BYTES=512*1024*1024
U64_MAX=2**64-1

class AdmissionError(Exception):
    def __init__(self,kind,detail,**fields): self.kind=kind; self.fields=fields; super().__init__(detail)

def _split(name):
    """weaver-spu artifact.rs `split_pattern`, ported: `<stem>-NNNNN-of-NNNNN` before a
    `.gguf` or `.safetensors` suffix, both fields five ASCII digits, the index within the
    count. Answers (stem, count, suffix), or None for a name outside the pattern."""
    for suffix in ('.gguf','.safetensors'):
        if name.endswith(suffix):
            rest=name[:-len(suffix)]
            break
    else:
        return None
    if len(rest)<15: return None
    head,of_part=rest[:-9],rest[-9:]
    if not of_part.startswith('-of-'): return None
    count_digits=of_part[4:]
    if not all(c in '0123456789' for c in count_digits): return None
    count=int(count_digits)
    if len(head)<6: return None
    stem_dash,index_digits=head[:-5],head[-5:]
    if not all(c in '0123456789' for c in index_digits): return None
    if not stem_dash.endswith('-'): return None
    stem=stem_dash[:-1]
    if not stem or count==0: return None
    index=int(index_digits)
    if index==0 or index>count: return None
    return stem,count,suffix

def containers(directory):
    """The files the Rust SPU pins for a directory artifact, ported from weaver-spu
    artifact.rs: `resolve`'s `container_within`, then `pin`. The containers are the
    regular files named `.gguf` or `.safetensors`, symbolic links followed. One of them
    resolves, or several resolve as one exactly where they are one split's shards, every
    shard present. Anything else is two artifacts in one directory and refuses as
    unresolvable. Answers the pinned files in shard order."""
    directory=Path(directory)
    try: names=os.listdir(directory)
    except OSError as e: raise AdmissionError('artifact_unreadable',f'{directory}: {e}') from None
    found=[directory/name for name in names
           if Path(name).suffix in ('.gguf','.safetensors') and os.path.isfile(directory/name)]
    if not found:
        raise AdmissionError('artifact_unresolvable',f'{directory} holds no container')
    if len(found)==1:
        first=found[0]
    else:
        parsed=[_split(p.name) for p in found]
        if None in parsed or len(set(parsed))!=1 or len(found)!=parsed[0][1]:
            raise AdmissionError('artifact_unresolvable',
                                 f'{directory} holds {len(found)} containers that are not one split')
        stem,count,suffix=parsed[0]
        first=directory/f'{stem}-{1:05}-of-{count:05}{suffix}'
    split=_split(first.name)
    return [first] if split is None else [
        directory/f'{split[0]}-{index:05}-of-{split[1]:05}{split[2]}' for index in range(1,split[1]+1)]

def split_members(path):
    """weaver-spu artifact.rs `pin` for a reference naming a file: a name in the split
    pattern is one shard of a set, so every shard is pinned, first shard first, whichever
    the operator named; any other name is itself. A shard that is absent is the pin's
    `artifact_unresolvable`."""
    path=Path(path); split=_split(path.name)
    if split is None: return [path]
    stem,count,suffix=split
    return [path.parent/f'{stem}-{index:05}-of-{count:05}{suffix}' for index in range(1,count+1)]

def pin(members):
    """weaver-spu artifact.rs `pin`, ported: each container opened once, O_NONBLOCK so a
    FIFO cannot block the open, its kind judged on the descriptor it opened rather than
    on the name. **Everything after reads through these descriptors**, the size, the
    load and the hash's container bytes, so the three cannot observe three different
    files. Answers (name, descriptor) pairs in shard order, the caller closing them."""
    pinned=[]
    try:
        for member in members:
            try: fd=os.open(member,os.O_RDONLY|os.O_NONBLOCK|os.O_CLOEXEC)
            except FileNotFoundError: raise AdmissionError('artifact_unresolvable',f'{member} is absent') from None
            except OSError as e: raise AdmissionError('artifact_unreadable',f'{member}: {e}') from None
            pinned.append((Path(member).name,fd))
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise AdmissionError('artifact_unresolvable',f'{member} is not a file')
    except BaseException:
        for _,fd in pinned: os.close(fd)
        raise
    return pinned

def pinned_size(pinned):
    return sum(os.fstat(fd).st_size for _,fd in pinned)

def pinned_tensors(pinned):
    """The containers' tensors, read through the pinned descriptors only."""
    from safetensors import safe_open
    tensors={}
    for _,fd in pinned:
        with safe_open(f'/proc/self/fd/{fd}',framework='pt') as f:
            for key in f.keys(): tensors[key]=f.get_tensor(key)
    return tensors

def file_digest(pinned):
    """weaver-spu artifact.rs `weights_hash` for a reference naming a file: the pinned
    descriptors' bytes in shard order, names excluded, in blake3. A read that fails is
    unreadable."""
    digest=blake3()
    for _,fd in pinned:
        at=0
        while True:
            chunk=os.pread(fd,1<<20,at)
            if not chunk: break
            digest.update(chunk); at+=len(chunk)
    return digest.hexdigest()

def weights_digest(path,pinned):
    """weaver-spu artifact.rs `hash_canonical`'s value: every regular file under the
    directory, sorted, symbolic links not followed, its relative path and then its
    bytes, in blake3. **A pinned container's bytes are read through its pin**, so under
    a swap during admission the digest names the bytes the load served, where the Rust
    walk reads the new name. A container reached through a symbolic link is left out of
    the walk, as the Rust leaves it, the pin still serving its size and its load: that
    parity is #25's symlinked-member item, awaiting the operator's ruling. A pinned
    name that no longer exists at all, unlinked after the pin, is refused rather than
    left out of the identity."""
    path=Path(path); pins=dict(pinned); met=set()
    digest=blake3()
    def walk(directory):
        for file in sorted(directory.iterdir(),key=lambda p:p.name):
            if file.is_symlink(): continue
            if file.is_dir(): yield from walk(file)
            elif file.is_file(): yield file
    for file in walk(path):
        relative=str(file.relative_to(path))
        digest.update(relative.encode())
        if relative in pins:
            met.add(relative); fd=pins[relative]; offset=0
            while chunk:=os.pread(fd,1024*1024,offset):
                digest.update(chunk); offset+=len(chunk)
        else:
            with file.open('rb') as stream:
                for chunk in iter(lambda:stream.read(1024*1024),b''): digest.update(chunk)
    absent=[]
    for name in sorted(set(pins)-met):
        try: os.lstat(path/name)
        except FileNotFoundError: absent.append(name)
    if absent: raise AdmissionError('artifact_unreadable',f'pinned and no longer named: {", ".join(absent)}')
    return digest.hexdigest()

def judge_room(ordinal,free,total,shard_bytes,headroom):
    """weaver-spu gpu/mod.rs `room_and_reach`, its room half, ported: a device admits
    when its free memory is at least the shard plus the headroom, the sum saturating at
    u64. It refuses and never evicts. The Rust SPU's `NoRoom` crosses the wire as the bare
    `device_cannot_admit`, so the figures travel in the refusal's detail, never as wire
    fields."""
    needed=min(shard_bytes+headroom,U64_MAX)
    if free<needed:
        raise AdmissionError('device_cannot_admit',
                             f'no room on device {ordinal}: free {free}, needed {needed}, total {total}')

# serde_json's recursion limit: its counter starts here and refuses on reaching zero,
# so the deepest nesting it reads is one less.
SERDE_DEPTH=128

def strict_json(data):
    """JSON as serde_json reads it, its whole refusal set: UTF-8 only; no `NaN`,
    `Infinity` or `-Infinity`; no number outside f64's range, an overflowing literal
    such as `1e400` included, which CPython reads as infinity; no unpaired surrogate
    escape; and no nesting past serde_json's recursion limit of 128 arrays and objects,
    which CPython reads until its own stack gives out. Raises ValueError for any of
    them."""
    import json
    text=data.decode('utf-8') if isinstance(data,bytes) else data
    def constant(name): raise ValueError(f'non-standard JSON constant {name}')
    def finite(literal):
        value=float(literal)
        if not math.isfinite(value): raise ValueError(f'number out of range: {literal}')
        return value
    def integer(literal):
        # serde_json reads an integer past i64 and u64 as an f64, out of range past it.
        value=int(literal)
        try: float(value)
        except OverflowError: raise ValueError(f'number out of range: {literal}') from None
        return value
    depth=0; quoted=False; escaped=False
    for c in text:
        if quoted:
            if escaped: escaped=False
            elif c=='\\': escaped=True
            elif c=='"': quoted=False
        elif c=='"': quoted=True
        elif c in '[{':
            depth+=1
            if depth>=SERDE_DEPTH: raise ValueError('nested past serde_json\'s recursion limit')
        elif c in ']}': depth-=1
    try: value=json.loads(text,parse_constant=constant,parse_float=finite,parse_int=integer)
    except RecursionError: raise ValueError('nested past the parser\'s depth') from None
    def check(item):
        if isinstance(item,str):
            if any(0xD800<=ord(c)<=0xDFFF for c in item): raise ValueError('unpaired surrogate')
        elif isinstance(item,dict):
            for key,inner in item.items(): check(key); check(inner)
        elif isinstance(item,list):
            for inner in item: check(inner)
    check(value)
    return value

class _Header:
    """A reader over a pinned descriptor, every short read unreadable, as
    weaver-spu artifact.rs's `read_exact` maps its failures."""
    def __init__(self,fd): self.fd=fd; self.at=0
    def take(self,n):
        # `read_exact`: a short read is read on from where it stopped until n bytes or
        # the end, an interrupted read retried (CPython retries EINTR itself, PEP 475).
        # The bytes gather in one growing buffer, so a filesystem that caps each read
        # small costs a copy of each chunk once, not of the whole prefix per chunk.
        data=bytearray()
        while len(data)<n:
            try: chunk=os.pread(self.fd,n-len(data),self.at+len(data))
            except OSError as e: raise AdmissionError('artifact_unreadable',f'the header does not read: {e}') from None
            if not chunk: raise AdmissionError('artifact_unreadable','the header is short')
            data+=chunk
        self.at+=n; return bytes(data)
    def u32(self): return int.from_bytes(self.take(4),'little')
    def u64(self): return int.from_bytes(self.take(8),'little')
    def text(self):
        length=self.u64()
        if length>64*1024: raise AdmissionError('artifact_unreadable','a GGUF string past its cap')
        try: return self.take(length).decode('utf-8')
        except UnicodeDecodeError: raise AdmissionError('artifact_unreadable','a GGUF string not UTF-8') from None
    def value(self,kind,depth=0):
        widths={0:1,1:1,2:2,3:2,4:4,5:4,10:8,11:8}
        if kind in widths: return int.from_bytes(self.take(widths[kind]),'little')
        if kind in (6,7,12): self.take({6:4,7:1,12:8}[kind]); return None
        if kind==8: return self.text()
        if kind==9:
            if depth>=8: raise AdmissionError('artifact_unreadable','a GGUF array nested past its cap')
            element=self.u32(); count=self.u64()
            if count>1024*1024: raise AdmissionError('artifact_unreadable','a GGUF array past its cap')
            for _ in range(count): self.value(element,depth+1)
            return None
        raise AdmissionError('artifact_unreadable',f'a GGUF value of unknown type {kind}')

def sidecar_dir_of(fd):
    """weaver-spu artifact.rs `sidecar_dir_of`, ported: the directory the kernel now
    names the pinned container's inode in, read back through `/proc/self/fd`, or None
    where the link does not read. **The sidecars are the pinned file's neighbours, not
    the name's**: a container reached through a link reads the `config.json` beside
    its target, as the Rust reads it, never one beside the link."""
    try: return Path(os.readlink(f'/proc/self/fd/{fd}')).parent
    except OSError: return None

U32_MAX=2**32-1

def read_eos(declared):
    """weaver-spu decoder/native.rs `read_eos`, ported over the declaration already
    read: `eos_token_id` as a scalar, or the first of a list, each as serde_json's
    `as_u64` takes it, so a bool, a float, a negative and an integer past u64 are no
    id. None declared refuses, as does one past the wire's u32 token width, never
    truncated into another token. Both are the native load's `LoadFailed`,
    `device_cannot_admit`."""
    eos=declared.get('eos_token_id') if isinstance(declared,dict) else None
    def as_u64(value): return value if type(value) is int and 0<=value<=U64_MAX else None
    found=as_u64(eos)
    if found is None and isinstance(eos,list) and eos: found=as_u64(eos[0])
    if found is None: raise AdmissionError('device_cannot_admit','config.json: no eos_token_id declared')
    if found>U32_MAX: raise AdmissionError('device_cannot_admit',f'config.json: eos_token_id {found} exceeds the token width')
    return found

def load_dir(fd):
    """weaver-spu decoder/native.rs `sidecar_dir`, ported: the pinned container's
    directory as the kernel names it, or where the link does not read, the parent of
    the descriptor's own `/proc/self/fd` path, which holds no `config.json`, so the
    load fails there as the native load does."""
    path=f'/proc/self/fd/{fd}'
    try: real=os.readlink(path)
    except OSError: real=path
    return Path(real).parent

def _sidecar(directory,name):
    """artifact.rs `read_sidecar_json`: absent is None, present and not JSON, as
    serde_json reads it, refuses. No directory, the pin's link not reading, is no
    sidecar, as the Rust's `None` is."""
    if directory is None: return None
    target=Path(directory)/name
    if not target.is_file(): return None
    try: return strict_json(target.read_bytes())
    except (OSError,ValueError): raise AdmissionError('artifact_unreadable',f'{name} does not read') from None

def read_header(fd):
    """weaver-spu artifact.rs `read_header`, step two, ported over the pinned first
    container: GGUF's magic, version, counts and key-value walk with their caps, the
    family from `general.architecture`; otherwise safetensors' length-prefixed JSON,
    the family from `__metadata__` or the sidecar `config.json`, the chat template
    from `tokenizer_config.json`, both sidecars refusing where present and unreadable.
    Every failure is unreadable. Answers the container, the family and the template.
    The sidecars are read beside the pinned file, `sidecar_dir_of`, as the Rust reads
    them."""
    sidecars=sidecar_dir_of(fd)
    reader=_Header(fd)
    if reader.take(4)==b'GGUF':
        reader.u32(); reader.u64(); count=reader.u64()
        if count>4096: raise AdmissionError('artifact_unreadable','GGUF metadata past its cap')
        family=template=None
        for _ in range(count):
            key=reader.text(); value=reader.value(reader.u32())
            if key=='general.architecture' and isinstance(value,str): family=value
            if key=='tokenizer.chat_template' and isinstance(value,str): template=value
        if family is None: raise AdmissionError('artifact_unreadable','GGUF names no architecture')
        return {'container':'gguf','family':family,'template':template}
    reader.at=0
    length=reader.u64()
    if length==0 or length>100*1024*1024: raise AdmissionError('artifact_unreadable','safetensors header length')
    try: parsed=strict_json(reader.take(length))
    except ValueError: raise AdmissionError('artifact_unreadable','safetensors header is not JSON') from None
    metadata=parsed.get('__metadata__') if isinstance(parsed,dict) else None
    config=_sidecar(sidecars,'config.json')
    family=None
    if isinstance(metadata,dict):
        family=metadata.get('architecture',metadata.get('model_type'))
    if not isinstance(family,str):
        family=config.get('model_type') if isinstance(config,dict) else None
    if not isinstance(family,str): raise AdmissionError('artifact_unreadable','no family declared')
    tokenizer_config=_sidecar(sidecars,'tokenizer_config.json')
    template=tokenizer_config.get('chat_template') if isinstance(tokenizer_config,dict) else None
    return {'container':'safetensors','family':family,
            'template':template if isinstance(template,str) else None}

# weaver-spu family/mod.rs `REGISTRY`, in its order: each entry's family, the device
# widths it declares and whether it taps the readout. A family named twice is contested
# between entries told apart by the chat template. tests/test_load_kinds.py holds this
# to the registry by the oracle.
REGISTRY=[
    ('llama',(1,2),False),('llama',(1,2),False),('llama',(1,2),False),
    ('qwen2',(1,2),True),('qwen3',(1,2),True),('qwen3moe',(1,2),False),
    ('qwen35',(1,2),False),('qwen35moe',(1,2),True),('gemma4',(1,2),False),
    ('nemotron_h_moe',(1,2),False),('mistral3',(1,2),False),('phi3',(1,2),False),
    ('phi3',(1,2),False),('gpt-oss',(1,2),False)]

# **The admission's steps, in the Rust SPU's order, and the kind each refuses as**,
# weaver-spu residency.rs `admit` and decoder/native.rs `ResidentModel::load`, per
# python-spu-Spec section 3.1's step table, which this list follows. A failure inside a
# step, anticipated or not, crosses as that step's kind.
ADMISSION_STEPS={
    'resolve':'artifact_unreadable',   # artifact.rs `resolve`: absent is unresolvable, raised
                                       # as such, and any other failed look unreadable
    'pin':'artifact_unreadable',       # artifact.rs `pin`: an absent shard unresolvable, raised
    'header':'artifact_unreadable',    # artifact.rs `read_header`, step two
    'select':'artifact_unreadable',    # family `select`: UnknownFamily, TemplateAbsent
    'width':'device_cannot_admit',     # family `judge_width`: WidthNotDeclared
    'readout':'device_cannot_admit',   # readout `judge`: NotTappable
    'distinct':'device_cannot_admit',  # residency `judge_distinct`: DuplicateDevice
    'size':'artifact_unreadable',      # the pinned size, `on_artifact` "size"
    'room':'device_cannot_admit',      # `judge_room_and_reach`: Device, DeviceRefused
    'hash':'artifact_unreadable',      # artifact.rs `weights_hash`
    'load':'device_cannot_admit',      # `load`: BackendNotBuilt, LoadFailed
}

def select(header):
    """family/mod.rs `select` over the registry above: no entry by the key is
    `UnknownFamily`, a contested family with no template `TemplateAbsent`. A contested
    family's template is matched in the Rust SPU by llama.cpp's renderer, which this
    build does not carry, so its candidates stand together. Answers the candidates."""
    from .family import same_key
    candidates=[entry for entry in REGISTRY if same_key(entry[0],header['family'])]
    if not candidates: raise AdmissionError('artifact_unreadable',f"no family {header['family']}")
    if len(candidates)>1 and header['template'] is None:
        raise AdmissionError('artifact_unreadable',f"{header['family']} is contested and names no template")
    return candidates

def resolve_directory(path):
    """weaver-spu artifact.rs `resolve`, its first look, ported: nothing at the path, or
    a path through a non-directory, is unresolvable; a lookup the kernel refuses is a
    present artifact this identity cannot reach, unreadable; anything but a directory or
    a regular file is unresolvable. Answers 'directory' or 'file': **a regular file
    resolves in the Rust SPU and is a shape this build does not serve**, refused at the
    load as the Rust SPU's `BackendNotBuilt` is, after the steps before it."""
    try: mode=os.stat(path).st_mode
    except (FileNotFoundError,NotADirectoryError):
        raise AdmissionError('artifact_unresolvable',str(path)) from None
    except OSError as e: raise AdmissionError('artifact_unreadable',f'{path}: {e}') from None
    if stat.S_ISDIR(mode): return 'directory'
    if stat.S_ISREG(mode): return 'file'
    raise AdmissionError('artifact_unresolvable',str(path))

def room_and_reach(devices,shard_bytes,headroom,cuda):
    """weaver-spu gpu/mod.rs `room_and_reach`, ported whole: every assigned ordinal is a
    device the driver answers for (`Unreachable`), every device has room for its shard
    and the headroom (`NoRoom`), and every ordered pair reaches the other
    (`NoPeerAccess`), both directions asked since the driver does not promise symmetry.
    Each refuses `device_cannot_admit`, the figures in the detail. `cuda` is torch's
    device layer, passed so a test can stand one up."""
    for ordinal in devices:
        if not cuda.is_available() or not 0<=ordinal<cuda.device_count():
            raise AdmissionError('device_cannot_admit',f'device {ordinal} is unreachable')
    for ordinal in devices:
        try: free,total=cuda.mem_get_info(ordinal)
        except RuntimeError as e: raise AdmissionError('device_cannot_admit',f'device {ordinal} is unreachable: {e}') from None
        judge_room(ordinal,free,total,shard_bytes,headroom)
    for source in devices:
        for target in devices:
            if source!=target and not cuda.can_device_access_peer(source,target):
                raise AdmissionError('device_cannot_admit',f'device {source} cannot reach device {target}')

class HFEngine:
    def __init__(self,artifact,devices,cpu=False,readout=False,headroom=HEADROOM_BYTES):
        import torch
        from transformers import AutoConfig, MODEL_FOR_CAUSAL_LM_MAPPING
        from tokenizers import Tokenizer
        self.torch=torch
        self.model=None; self.cache=None; self.hooks=[]; self.pinned=[]
        # Whether a load began on the device, the one thing close() has to free there.
        self.placing=False
        self.logits=None; self.norms=[]; self.current_norms=[]; self.readout=readout
        path=Path(artifact)
        # The steps of weaver-spu residency.rs `admit` in its order, each named, so a
        # failure inside one crosses as that step's kind, per `ADMISSION_STEPS`.
        model=None; failure=None; step='resolve'
        try:
            reference=resolve_directory(path)
            members=containers(path) if reference=='directory' else split_members(path)
            step='pin'; self.pinned=pin(members)
            step='header'
            header=read_header(self.pinned[0][1])
            step='select'; candidates=select(header)
            step='width'
            if not any(len(devices) in widths for _,widths,_ in candidates):
                raise AdmissionError('device_cannot_admit',f'{len(devices)} devices is a width {header["family"]} does not declare')
            step='readout'
            if readout and not any(taps for _,_,taps in candidates):
                raise AdmissionError('device_cannot_admit',f'{header["family"]} does not tap the readout')
            step='distinct'
            if len(set(devices))!=len(devices): raise AdmissionError('device_cannot_admit','a device is named twice')
            step='size'; shard_bytes=pinned_size(self.pinned)//len(devices)
            # **The room is the device's**, judged before the weights load, as the Rust
            # SPU judges it. A CPU experiment has no device and so no room to judge.
            step='room'
            if cpu and devices!=[0]: raise AdmissionError('device_cannot_admit','CPU experiment requires ordinal 0')
            self.device='cpu' if cpu else f'cuda:{devices[0]}'
            if not cpu: room_and_reach(devices,shard_bytes,headroom,torch.cuda)
            # The hash before any device is taken, as weaver-spu-Spec section 3 places it.
            # The kind is the resolution's, as weaver-spu `resolve_with_kind` carries it.
            step='hash'
            self.weights_hash=(weights_digest(path,self.pinned) if reference=='directory'
                               else file_digest(self.pinned))
            # Step four, decoder/native.rs `ResidentModel::load` in its order. A GGUF
            # container or a file reference is a backend this build does not carry.
            step='load'
            if reference=='file' or header['container']=='gguf':
                raise AdmissionError('device_cannot_admit',f"this build carries no backend for {header['container']} {header['family']}")
            if len(devices)!=1: raise AdmissionError('device_cannot_admit','this build serves one device')
            # `read_declaration` then `judge_family` on the declaration's own
            # `model_type`, BackendDoesNotServe crossing unreadable, the one step-four
            # refusal that does.
            # The sidecars beside the pinned container, as the native load reads them.
            beside=load_dir(self.pinned[0][1])
            declared=strict_json((beside/'config.json').read_bytes())
            from .family import SERVED_ARCHITECTURE,same_key
            model_type=declared.get('model_type') if isinstance(declared,dict) else None
            if not isinstance(model_type,str) or not same_key(model_type,SERVED_ARCHITECTURE):
                raise AdmissionError('artifact_unreadable',f'the native backend does not serve {model_type}')
            # `read_config`: a quantized artifact is one the engine cannot take.
            config=AutoConfig.from_pretrained(beside,local_files_only=True,trust_remote_code=False)
            if getattr(config,'quantization_config',None):
                raise AdmissionError('device_cannot_admit','quantized artifacts unsupported')
            self.max_context=config.max_position_embeddings
            # `read_eos`, after `read_config` and before the tokenizer, as the native
            # load orders them.
            self.declared_eos=read_eos(declared)
            self.tokenizer=Tokenizer.from_file(str(beside/'tokenizer.json'))
            self.terminator=self.tokenizer.token_to_id('<|im_end|>')
            # This build's own judgment, the renderer's markers each one token.
            for marker in ('<|im_start|>','<|im_end|>'):
                ids=self.tokenizer.encode(marker,add_special_tokens=False).ids
                if len(ids)!=1 or self.tokenizer.id_to_token(ids[0])!=marker:
                    raise AdmissionError('artifact_unreadable',f'marker not promoted: {marker}')
            # The weights come through the pins only. The concrete class for the config,
            # from transformers' own mapping, takes them as a state dict, so the model
            # code, its tying and its cast are the path load's, and the bytes are not.
            model,loaded=MODEL_FOR_CAUSAL_LM_MAPPING[type(config)].from_pretrained(None,config=config,
                state_dict=pinned_tensors(self.pinned),dtype=getattr(torch,DTYPE),
                attn_implementation='eager',output_loading_info=True)
            # **A weight the model needs and the artifact lacks refuses**, as candle's
            # VarBuilder refuses a missing tensor, `LoadFailed`, where transformers would
            # initialise it at random and serve a model the artifact never held.
            if loaded['missing_keys']:
                raise AdmissionError('device_cannot_admit',f"missing tensors: {', '.join(sorted(loaded['missing_keys'])[:5])}")
            # Set where placement begins, so a move that fails part-way is still freed.
            self.placing=not cpu
            self.model=model.to(self.device).eval()
            self.layers=config.num_hidden_layers
            if readout:
                for layer in self.model.model.layers:
                    self.hooks.append(layer.register_forward_hook(self._tap))
            self._unpin()
            self.artifact=str(path.resolve())
            torch.set_num_threads(1)
            torch.use_deterministic_algorithms(True)
        except AdmissionError as e:
            failure=e.with_traceback(None)
        except torch.OutOfMemoryError as e:
            failure=AdmissionError('device_cannot_admit',str(e))
        except Exception as e:
            failure=AdmissionError(ADMISSION_STEPS[step],f'{step}: {e}')
        # **A move that fails part-way is released.** Inside the except block the local
        # model and the exception's traceback, whose frames hold the module being moved,
        # keep its device tensors alive, and freeing the cache there frees nothing. Out
        # of it, with the local dropped and no cause chained, close() collects and frees.
        if failure is not None:
            failure.__context__=failure.__cause__=None
            model=None
            self.close()
            raise failure
    def _tap(self,module,args,output):
        values=output[0] if isinstance(output,tuple) else output
        norm=values.detach().float().square().sum().sqrt().item()
        self.current_norms.append(norm)
    def tokenize(self,text): return self.tokenizer.encode(text,add_special_tokens=False).ids
    def detokenize(self,tokens):
        return self.tokenizer.decode(tokens,skip_special_tokens=False)
    def append(self,tokens):
        if not tokens: return
        torch=self.torch
        self.current_norms=[]
        with torch.inference_mode():
            result=self.model(input_ids=torch.tensor([tokens],device=self.device),
                              past_key_values=self.cache,use_cache=True)
        self.cache=result.past_key_values
        self.logits=result.logits[0,-1].float().cpu().tolist()
        if self.readout:
            if len(self.current_norms)!=self.layers: raise RuntimeError('incomplete residual tap')
            self.norms.extend(self.current_norms)
    def rebuild(self,tokens):
        self.cache=None; self.logits=None; self.norms=[]
        self.append(tokens)
    def _unpin(self):
        for _,fd in getattr(self,'pinned',[]): os.close(fd)
        self.pinned=[]
    def close(self):
        self._unpin()
        for hook in self.hooks: hook.remove()
        self.hooks=[]; self.cache=None; self.model=None; self.logits=None
        gc.collect()
        # **The device is touched only where a load began on it.** A refusal before the
        # load, the room judgment's among them, placed nothing, so there is nothing to
        # free, and asking the driver would initialise a context on a card this
        # admission never used.
        if getattr(self,'placing',False):
            self.placing=False
            with self.torch.cuda.device(self.device): self.torch.cuda.empty_cache()
