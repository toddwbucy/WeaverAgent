import json
import os
import subprocess
from pathlib import Path
import pytest
ROOT=Path(__file__).resolve().parents[1]
# The child processes the tests spawn import python_spu from this tree: the locked
# environment holds the third-party packages alone, python-spu shipping as its zipapp.
os.environ['PYTHONPATH']=os.pathsep.join(filter(None,[str(ROOT/'src'),os.environ.get('PYTHONPATH')]))
@pytest.fixture(scope='session')
def oracle():
    executable=ROOT/'oracle/target/debug/python-spu-oracle'
    if not executable.exists(): pytest.fail('build the Rust oracle before running tests')
    process=subprocess.Popen([str(executable)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
    def call(**request):
        process.stdin.write(json.dumps(request)+'\n'); process.stdin.flush()
        line=process.stdout.readline()
        assert line,'Rust oracle terminated'
        return json.loads(line)
    yield call
    process.stdin.close()
    try: process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill(); process.wait()
        raise

@pytest.fixture
def instruction():
    from python_spu.wire import SpuInstruction
    return SpuInstruction.model_validate({'decoder':{
        'model-binding':{'artifact':'fixture','devices':[0]},
        'residual-readout-election':False,'surprisal-election':True,
        'refeed-permission':True,'identity':[],
        'tunable-values':{'seed':11,'context-capacity':256,'max-tokens-per-turn':4}}})

@pytest.fixture(scope='session')
def tiny_model(tmp_path_factory):
    import torch
    from transformers import Qwen2Config,Qwen2ForCausalLM,PreTrainedTokenizerFast
    from tokenizers import Tokenizer,models,pre_tokenizers
    path=tmp_path_factory.mktemp('tiny-qwen2')
    vocab={'[UNK]':0,'<|im_start|>':1,'<|im_end|>':2,'system':3,'user':4,
           'assistant':5,'hello':6,'world':7,'be':8,'precise':9}
    vocab.update({f'word{i}':i for i in range(10,64)})
    backend=Tokenizer(models.WordLevel(vocab,unk_token='[UNK]'))
    backend.pre_tokenizer=pre_tokenizers.WhitespaceSplit()
    tokenizer=PreTrainedTokenizerFast(tokenizer_object=backend,unk_token='[UNK]',
        eos_token='<|im_end|>',additional_special_tokens=['<|im_start|>','<|im_end|>'])
    tokenizer.save_pretrained(path)
    torch.manual_seed(726)
    config=Qwen2Config(vocab_size=64,hidden_size=32,intermediate_size=64,
        num_hidden_layers=2,num_attention_heads=4,num_key_value_heads=2,
        max_position_embeddings=256,bos_token_id=1,eos_token_id=2)
    Qwen2ForCausalLM(config).save_pretrained(path)
    return path
