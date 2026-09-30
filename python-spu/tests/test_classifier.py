import json
import pytest
from python_spu.classifier import Classifier
from python_spu.session import Refusal

@pytest.fixture(scope='module')
def artifact(tmp_path_factory,tiny_model):
    import torch
    from transformers import ModernBertConfig,ModernBertForSequenceClassification,AutoTokenizer
    path=tmp_path_factory.mktemp('modernbert')
    import shutil
    for name in ('tokenizer.json','tokenizer_config.json'):
        shutil.copyfile(tiny_model/name,path/name)
    torch.manual_seed(726)
    config=ModernBertConfig(vocab_size=64,hidden_size=32,intermediate_size=64,
        num_hidden_layers=2,num_attention_heads=4,max_position_embeddings=64,
        local_attention=16,global_attn_every_n_layers=2,pad_token_id=0,bos_token_id=1,eos_token_id=2,cls_token_id=1,sep_token_id=2,
        id2label={0:'safe',1:'unsafe'},label2id={'safe':0,'unsafe':1},reference_compile=False)
    ModernBertForSequenceClassification(config).save_pretrained(path)
    return path

def test_classifier_stateless_scores_and_wire(artifact,oracle):
    c=Classifier(artifact,0,cpu=True)
    try:
        first=c.classify('hello world','t')
        c.classify('be precise','other')
        assert c.classify('hello world','t')==first
        labels=first['body']['labels']
        assert [x['label'] for x in labels]==['safe','unsafe']
        assert sum(x['score'] for x in labels)==pytest.approx(1)
        assert oracle(op='round',type='LabelAnswer',body=first)['ok']['body']['turn']=='t'
        with pytest.raises(Refusal,match='oversized'): c.classify('hello '*65)
        assert c.classify('hello world','t')==first
    finally: c.close()
