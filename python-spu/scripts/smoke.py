"""Run a trained local model through actual inherited sockets; save evidence."""
import argparse
import json
import os
from pathlib import Path
import platform
import socket
import stat
import subprocess
import sys
import time
import tomllib

# The package from this tree, for this process and for the SPU children it launches, as
# scripts/declare_imports.py reaches it, so the README's command runs as written.
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'src'))
os.environ['PYTHONPATH']=os.pathsep.join(filter(None,[str(ROOT/'src'),os.environ.get('PYTHONPATH')]))
from python_spu.client import LocalProcess  # noqa: E402
from python_spu.engine import DTYPE  # noqa: E402

parser=argparse.ArgumentParser()
parser.add_argument('artifact',type=Path)
parser.add_argument('--output',type=Path,default=Path('results/trained-smoke.json'))
parser.add_argument('--device',choices=('cpu','cuda'),default='cpu')
args=parser.parse_args()
# The model is read by stat before anything is written, so a missing, misspelled or
# unreadable artifact refuses by name rather than leaving a report directory behind a
# refused admission.
try:
    if not stat.S_ISDIR(os.stat(args.artifact).st_mode):
        raise ValueError(f'{args.artifact} is not a directory')
except (OSError,ValueError) as error:
    sys.exit(f'smoke: {error}')
args.output.parent.mkdir(parents=True,exist_ok=True)
if args.device=='cuda':
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')
import torch,transformers
if args.device=='cuda' and not torch.cuda.is_available():
    raise RuntimeError('CUDA requested but unavailable; no CPU fallback')
def gpu_status():
    if args.device!='cuda': return None
    return subprocess.check_output(['nvidia-smi',
        '--query-gpu=index,uuid,name,driver_version,memory.used,memory.total',
        '--format=csv'],text=True,timeout=60).strip()
gpu_before=gpu_status()
identity=[{'role':'system','content':[{'type':'text','text':'You are a concise, helpful assistant.'}]}]
delta=[{'role':'user','content':[{'type':'text','text':'What is 2 + 2? Answer in one short sentence.'}]}]
runs=[]
for run,readout in enumerate((False,False,True)):
    print(f'Run {run+1}/3: readout={readout}',flush=True)
    process=LocalProcess(args.output.parent/f'{args.output.stem}-{run}.stderr',
                         arguments=('--cpu-experiment',) if args.device=='cpu' else ())
    try:
        instruction={'decoder':{'model-binding':{'artifact':str(args.artifact.resolve()),'devices':[0]},
            'residual-readout-election':readout,'surprisal-election':True,'identity':identity,
            'tunable-values':{'seed':11,'context-capacity':1024,'max-tokens-per-turn':32}}}
        begin=time.perf_counter()
        answer=process.ask({'kind':'admit','instruction':instruction})
        assert answer['payload']=={'kind':'answer','body':{'kind':'admitted'}},answer
        load_seconds=time.perf_counter()-begin
        gpu_loaded=gpu_status()
        decode=process.channels[1]
        decode.send({'kind':'open','session':'smoke','messages':identity})
        assert decode.receive()=={'kind':'opened'}
        decode.send({'kind':'append_and_generate','turn':'smoke-turn','delta':delta})
        frames=[]; begin=time.perf_counter()
        while True:
            answer=decode.receive(); frames.append(answer)
            if answer['kind']=='generated': break
            assert answer['kind'] in ('token','field','column'),answer
        elapsed=time.perf_counter()-begin
        close=frames[-1]['body']
        print(close['emission'],flush=True)
        assert close['emission']==''.join(f['body']['piece'] for f in frames if f['kind']=='token')
        decode.send({'kind':'flush','keep':0}); flushed=decode.receive()
        assert flushed['kind']=='flushed'
        decode.sock.shutdown(socket.SHUT_RDWR)
        assert process.ask({'kind':'release'})['payload']=={'kind':'answer','body':{'kind':'released'}}
        runs.append({'readout':readout,'load_seconds':load_seconds,'generation_seconds':elapsed,
                     'generation':close,'flush':flushed,'gpu_loaded':gpu_loaded})
    finally:
        code=process.close()
        assert code==0,code
keys=('output_tokens','entropies','surprisals','perplexity')
for key in keys:
    assert runs[0]['generation']['measurement'][key]==runs[1]['generation']['measurement'][key],key
    assert runs[0]['generation']['measurement'][key]==runs[2]['generation']['measurement'][key],key
report={'oracle_revision':tomllib.loads((ROOT/'oracle'/'Cargo.toml').read_text())['dependencies']['weaver-spu']['rev'],
    'deployment':{'host':platform.node(),'python':platform.python_version(),'torch':torch.__version__,
      'transformers':transformers.__version__,'device':args.device,'dtype':DTYPE,
      'cuda_runtime':torch.version.cuda,
      'gpu_name':torch.cuda.get_device_name(0) if args.device=='cuda' else None,
      'cuda_visible_devices':os.environ.get('CUDA_VISIBLE_DEVICES'),
      'cublas_workspace_config':os.environ.get('CUBLAS_WORKSPACE_CONFIG'),
      'attention':'eager','threads':1,'sampler':'candle-chain, rand StdRng ChaCha12'},
    'gpu_before':gpu_before,'gpu_after':gpu_status(),
    'repeatability':'exact for output tokens, entropies, surprisals and perplexity across fresh processes',
    'readout_neutrality':'same quantities exact with tap enabled', 'runs':runs}
args.output.write_text(json.dumps(report,indent=2,ensure_ascii=False,allow_nan=False)+'\n')
print(f'Evidence saved to {args.output}',flush=True)
