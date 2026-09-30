"""Experimental separate label process, matching Rust's classifier topology."""
import argparse
import gc
import json
from pathlib import Path
import sys
from .engine import AdmissionError
from .session import Refusal
from .transport import adopt_channels,Closed,ChannelFault
from .wire import LABEL_DIRECTIVE,dump

class Classifier:
    def __init__(self,artifact,device,cpu=False):
        import torch
        from transformers import AutoConfig,AutoModelForSequenceClassification
        from tokenizers import Tokenizer
        self.torch=torch; self.model=None
        path=Path(artifact)
        if not path.is_dir(): raise AdmissionError('not_admitted','artifact directory missing')
        raw=json.loads((path/'config.json').read_text())
        labels=raw.get('id2label')
        if not isinstance(labels,dict) or not labels or set(labels)!={str(i) for i in range(len(labels))}:
            raise AdmissionError('not_admitted','artifact must declare contiguous id2label')
        if not all(isinstance(x,str) for x in labels.values()): raise AdmissionError('not_admitted','labels must be strings')
        config=AutoConfig.from_pretrained(path,local_files_only=True,trust_remote_code=False)
        if config.model_type!='modernbert': raise AdmissionError('not_admitted','only ModernBERT classifier is supported')
        if cpu and device!=0: raise AdmissionError('not_admitted','CPU experiment requires ordinal 0')
        self.device='cpu' if cpu else f'cuda:{device}'
        if not cpu and (not torch.cuda.is_available() or device>=torch.cuda.device_count()):
            raise AdmissionError('not_admitted','assigned CUDA device unavailable')
        self.labels=[labels[str(i)] for i in range(len(labels))]
        self.bound=config.max_position_embeddings
        self.tokenizer=Tokenizer.from_file(str(path/'tokenizer.json'))
        try:
            self.model=AutoModelForSequenceClassification.from_pretrained(path,local_files_only=True,
                trust_remote_code=False,dtype=torch.float32,attn_implementation='eager').to(self.device).eval()
            torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
        except Exception:
            self.close(); raise
    def classify(self,content,turn=None):
        try: tokens=self.tokenizer.encode(content,add_special_tokens=True).ids
        except Exception as e: raise Refusal('malformed_content') from e
        if len(tokens)>self.bound: raise Refusal('oversized',requested=len(tokens),bound=self.bound)
        if not tokens: raise Refusal('malformed_content')
        t=self.torch
        with t.inference_mode():
            ids=t.tensor([tokens],device=self.device)
            logits=self.model(input_ids=ids,attention_mask=t.ones_like(ids)).logits[0]
            scores=t.softmax(logits.float(),dim=-1)
        if len(scores)!=len(self.labels) or not t.isfinite(scores).all(): raise RuntimeError('invalid classifier head')
        return {'kind':'scored','body':{'turn':turn,'labels':[
            {'label':label,'score':score} for label,score in zip(self.labels,scores.cpu().tolist())]}}
    def close(self):
        self.model=None; gc.collect()
        if getattr(self,'device','cpu').startswith('cuda'):
            with self.torch.cuda.device(self.device): self.torch.cuda.empty_cache()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('artifact'); parser.add_argument('device',type=int)
    parser.add_argument('--cpu-experiment',action='store_true')
    args=parser.parse_args(); model=None
    try:
        channel=adopt_channels((3,))[0]
        try: model=Classifier(args.artifact,args.device,args.cpu_experiment)
        except Exception as e:
            channel.send({'kind':'not_admitted','reason':str(e)}); return 1
        channel.send({'kind':'ready'})
        while True:
            try: value=channel.receive()
            except Closed: return 0
            except ChannelFault:
                channel.send({'kind':'malformed_content'}); continue
            try: directive=dump(LABEL_DIRECTIVE.validate_python(value))
            except ValueError:
                channel.send({'kind':'malformed_content'}); continue
            try: answer=model.classify(directive['content'],directive['turn'])
            except Refusal as e: answer=e.payload
            except Exception as e:
                channel.send({'kind':'fault','body':{'case':'device_fault_during_generation','account':{'detail':str(e)}}})
                return 1
            channel.send(answer)
    except Exception as e:
        print(json.dumps({'classifier_fault':str(e)}),file=sys.stderr); return 1
    finally:
        if model is not None: model.close()
if __name__=='__main__': sys.exit(main())
