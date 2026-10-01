"""Local Qwen2, explicit placement, cached forwards and full rollback rebuild."""
import gc
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

class _Header:
    """A reader over a pinned descriptor, every short read unreadable, as
    weaver-spu artifact.rs's `read_exact` maps its failures."""
    def __init__(self,fd): self.fd=fd; self.at=0
    def take(self,n):
        data=os.pread(self.fd,n,self.at)
        if len(data)!=n: raise AdmissionError('artifact_unreadable','the header is short')
        self.at+=n; return data
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

def _sidecar(directory,name):
    """artifact.rs `read_sidecar_json`: absent is None, present and not JSON refuses."""
    import json
    target=Path(directory)/name
    if not target.is_file(): return None
    try: return json.loads(target.read_text())
    except (OSError,ValueError): raise AdmissionError('artifact_unreadable',f'{name} does not read') from None

def read_header(fd,sidecars):
    """weaver-spu artifact.rs `read_header`, step two, ported over the pinned first
    container: GGUF's magic, version, counts and key-value walk with their caps, the
    family from `general.architecture`; otherwise safetensors' length-prefixed JSON,
    the family from `__metadata__` or the sidecar `config.json`, the chat template
    from `tokenizer_config.json`, both sidecars refusing where present and unreadable.
    Every failure is unreadable. Answers the container, the family and the template."""
    import json
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
    try: parsed=json.loads(reader.take(length))
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

# weaver-spu family/mod.rs `REGISTRY`'s families, in its order, a family named twice
# being contested between entries told apart by the chat template. tests/test_load_kinds.py
# holds this to the registry by the oracle.
REGISTRY_FAMILIES=['llama','llama','llama','qwen2','qwen3','qwen3moe','qwen35','qwen35moe',
                   'gemma4','nemotron_h_moe','mistral3','phi3','phi3','gpt-oss']

def backend_not_built(header):
    """What the Rust SPU answers for a header this build has no backend for, judged as
    its admit judges it before step four: a family the registry does not hold is
    `UnknownFamily`, and a contested family with no template `TemplateAbsent`, both
    unreadable; any other passes the free steps and meets `BackendNotBuilt`,
    `device_cannot_admit`. A contested family's template is matched by llama.cpp's
    renderer in the Rust SPU, which this build does not carry, so a template it would
    match no entry by is answered here as it would be had it matched."""
    from .family import same_key
    matches=[f for f in REGISTRY_FAMILIES if same_key(f,header['family'])]
    if not matches: raise AdmissionError('artifact_unreadable',f"no family {header['family']}")
    if len(matches)>1 and header['template'] is None:
        raise AdmissionError('artifact_unreadable',f"{header['family']} is contested and names no template")
    raise AdmissionError('device_cannot_admit',f"this build carries no backend for {header['container']} {header['family']}")

def resolve_directory(path):
    """weaver-spu artifact.rs `resolve`, its first look, ported: nothing at the path, or
    a path through a non-directory, is unresolvable; a lookup the kernel refuses is a
    present artifact this identity cannot reach, unreadable; anything but a directory or
    a regular file is unresolvable. Answers 'directory' or 'file': **a regular file
    resolves in the Rust SPU and is a shape this build does not serve**, judged after its
    header by `backend_not_built`."""
    try: mode=os.stat(path).st_mode
    except (FileNotFoundError,NotADirectoryError):
        raise AdmissionError('artifact_unresolvable',str(path)) from None
    except OSError as e: raise AdmissionError('artifact_unreadable',f'{path}: {e}') from None
    if stat.S_ISDIR(mode): return 'directory'
    if stat.S_ISREG(mode): return 'file'
    raise AdmissionError('artifact_unresolvable',str(path))

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
        reference=resolve_directory(path)
        # Step one, as the Rust SPU's: the directory resolves to its containers, free.
        members=containers(path) if reference=='directory' else [path]
        # Step two, before any device is judged: the header through the pin. A GGUF
        # container, or a file reference, is a shape whose backend this build does not
        # carry, answered as the Rust SPU answers its header before step four.
        self.pinned=pin(members)
        try:
            header=read_header(self.pinned[0][1],path if reference=='directory' else path.parent)
            if reference=='file' or header['container']=='gguf': backend_not_built(header)
            if len(devices)!=1: raise AdmissionError('device_cannot_admit','one device required')
            if cpu and devices!=[0]: raise AdmissionError('device_cannot_admit','CPU experiment requires ordinal 0')
            self.device='cpu' if cpu else f'cuda:{devices[0]}'
            if not cpu and (not torch.cuda.is_available() or devices[0]>=torch.cuda.device_count()):
                raise AdmissionError('device_cannot_admit','assigned CUDA device unavailable')
        except BaseException:
            self._unpin(); raise
        # A refusal is recorded here and raised after the except block ends, so the frames
        # and the exception that hold a part-moved model are gone when the cache is freed.
        model=None; failure=None
        # **Which step a failure lands in decides its wire kind**, per python-spu-Spec
        # section 3.1, as weaver-spu residency.rs maps its own: a read of the artifact
        # or its sidecars is `Unreadable` and crosses `artifact_unreadable`; the engine
        # taking the weights is step four, `LoadFailed`, which crosses
        # `device_cannot_admit`.
        loading=False
        try:
            # The pin, taken above and held for the admit. The sidecars, config and
            # tokenizer, are opens by name, the limit weaver-spu's native `sidecar_dir`
            # states for its own.
            config=AutoConfig.from_pretrained(path,local_files_only=True,trust_remote_code=False)
            if config.model_type!='qwen2': raise AdmissionError('artifact_unreadable','only qwen2 is verified')
            # A quantized safetensors artifact is one the engine cannot take, which in the
            # Rust SPU fails inside the native load, step four.
            if getattr(config,'quantization_config',None):
                raise AdmissionError('device_cannot_admit','quantized artifacts unsupported')
            self.max_context=config.max_position_embeddings
            # The Rust SPU reads the vocabulary inside the native load, step four, where
            # a tokenizer.json that does not read is `LoadFailed` (decoder/native.rs).
            try: self.tokenizer=Tokenizer.from_file(str(path/'tokenizer.json'))
            except Exception as e: raise AdmissionError('device_cannot_admit',f'tokenizer.json: {e}') from None
            self.terminator=self.tokenizer.token_to_id('<|im_end|>')
            for marker in ('<|im_start|>','<|im_end|>'):
                ids=self.tokenizer.encode(marker,add_special_tokens=False).ids
                if len(ids)!=1 or self.tokenizer.id_to_token(ids[0])!=marker:
                    raise AdmissionError('artifact_unreadable',f'marker not promoted: {marker}')
            # **Room is judged before the weights load**, per python-spu-Spec section 3,
            # as the Rust SPU judges it: the shard is the pinned containers' size over
            # the device count. A CPU experiment has no device and so no room to judge.
            if not cpu:
                shard_bytes=pinned_size(self.pinned)//len(devices)
                try: free,total=torch.cuda.mem_get_info(devices[0])
                except RuntimeError as e: raise AdmissionError('device_cannot_admit',f'device {devices[0]} unreachable: {e}') from None
                judge_room(devices[0],free,total,shard_bytes,headroom)
            loading=True
            # The weights come through the pins only. The concrete class for the config,
            # from transformers' own mapping, takes them as a state dict, so the model
            # code, its tying and its cast are the path load's, and the bytes are not.
            model=MODEL_FOR_CAUSAL_LM_MAPPING[type(config)].from_pretrained(None,config=config,
                state_dict=pinned_tensors(self.pinned),dtype=getattr(torch,DTYPE),
                attn_implementation='eager')
            # Set where placement begins, so a move that fails part-way is still freed.
            self.placing=not cpu
            self.model=model.to(self.device).eval()
            self.layers=config.num_hidden_layers
            if readout:
                for layer in self.model.model.layers:
                    self.hooks.append(layer.register_forward_hook(self._tap))
            loading=False
            self.weights_hash=weights_digest(path,self.pinned)
            self._unpin()
            self.artifact=str(path.resolve())
            torch.set_num_threads(1)
            torch.use_deterministic_algorithms(True)
        except AdmissionError as e:
            failure=e.with_traceback(None)
        except torch.OutOfMemoryError as e:
            failure=AdmissionError('device_cannot_admit',str(e))
        except Exception as e:
            failure=AdmissionError('device_cannot_admit' if loading else 'artifact_unreadable',str(e))
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
