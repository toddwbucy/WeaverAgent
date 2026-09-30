use serde_json::{Value,json};
use weaver_spu::decoder::backend::{Backend,DecodeFault,FlushMechanism,TokenId};
use weaver_spu::decoder::session::{Session,NeverCancels,PositionedSinks,SamplerBuild,StopCondition};
struct Engine { logits: Vec<f32> }
impl Backend for Engine {
 fn decode_at(&mut self,_:&[TokenId],_:usize)->Result<(),DecodeFault>{Ok(())}
 fn distribution(&self)->Result<&[f32],DecodeFault>{Ok(&self.logits)}
 fn sample(&mut self)->Result<TokenId,DecodeFault>{Ok(TokenId(1))}
 fn reseed(&mut self,_:u64,_:&[TokenId])->Result<(),DecodeFault>{Ok(())}
 fn truncate_to(&mut self,_:usize)->Result<(),DecodeFault>{Ok(())}
 fn reestablish(&mut self)->Result<(),DecodeFault>{Ok(())}
 fn close(&mut self){}
}
fn tokens(value:&Value)->Vec<TokenId>{value.as_array().unwrap().iter().map(|v|TokenId(v.as_u64().unwrap() as u32)).collect()}
pub fn run(input:&Value)->Result<Value,String>{
 let mut session=Session::new(Box::new(Engine{logits:vec![-1000.,0.,-1000.]}),
    input["capacity"].as_u64().ok_or("capacity")? as usize,FlushMechanism::ReestablishAndReprefill,true);
 let mut results=vec![];
 for step in input["steps"].as_array().ok_or("steps")? {
    let result=match step["kind"].as_str().ok_or("kind")? {
      "open"=>session.open(&tokens(&step["tokens"])).map(|()|json!({"kind":"opened"})),
      "flush"=>session.flush(step["keep"].as_u64().unwrap() as usize).map(|()|json!({"kind":"flushed"})),
      "elide"=>session.elide(step["from"].as_u64().unwrap() as usize,step["to"].as_u64().unwrap() as usize).map(|_|json!({"kind":"elided"})),
      "generate"=>session.append_and_generate(&tokens(&step["tokens"]),
        &StopCondition{stop_tokens:vec![TokenId(2)],terminator:TokenId(2),max_tokens:step["limit"].as_u64().unwrap() as usize},
        &mut NeverCancels,&mut |_|{},PositionedSinks{field:None,on_column:&mut |_,_|{}},
        SamplerBuild{seed:11,penalty_window:64})
        .map(|g|json!({"kind":"generated","tokens":g.tokens.iter().map(|t|t.0).collect::<Vec<_>>()})),
      _=>return Err("unknown step".into()),
    };
    let outcome=match result {
        Ok(v)=>v,
        Err(DecodeFault::Overflow{resident,requested,capacity})=>json!({"kind":"overflow","resident":resident,"requested":requested,"capacity":capacity}),
        Err(DecodeFault::NotOpen)=>json!({"kind":"not_open"}),
        Err(DecodeFault::AlreadyOpen)=>json!({"kind":"out_of_order"}),
        Err(DecodeFault::UnremovableSpan{from,to,prefix,resident})=>json!({"kind":"unremovable_span","from":from,"to":to,"prefix":prefix,"resident":resident}),
        Err(e)=>return Err(format!("{e:?}")),
    };
    results.push(json!({"outcome":outcome,"resident":session.resident_len()}));
 }
 Ok(json!(results))
}
