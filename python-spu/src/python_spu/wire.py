"""Strict Python payloads checked against the pinned Rust serde implementation.

Serde's ordinary structs ignore unknown fields; configuration structs forbid them.
Validation and serialization are separate: explicit default booleans must survive,
while absent optional configuration members are omitted.
"""
from typing import Annotated, Literal, Union
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, create_model

U64 = Annotated[int, Field(ge=0, le=2**64-1, strict=True)]
U32 = Annotated[int, Field(ge=0, le=2**32-1, strict=True)]
Finite = Annotated[float, Field(allow_inf_nan=False)]

class Wire(BaseModel):
    model_config = ConfigDict(strict=True, extra='ignore')

class Config(Wire):
    model_config = ConfigDict(strict=True, extra='forbid', alias_generator=lambda s: s.replace('_', '-'), populate_by_name=False)

class Text(Wire):
    type: Literal['text']
    text: str
class ToolCall(Wire):
    type: Literal['tool_call']
    name: str
    arguments: str
class ToolResult(Wire):
    type: Literal['tool_result']
    content: str
Block = Annotated[Union[Text, ToolCall, ToolResult], Field(discriminator='type')]
class Message(Wire):
    role: Literal['system', 'user', 'assistant', 'tool_result']
    content: list[Block]
class ModelBinding(Config):
    artifact: str
    devices: list[U32]
class FieldElection(Config):
    depth: U32
class DecoderInstruction(Config):
    model_binding: ModelBinding
    residual_readout_election: bool
    field_election: FieldElection | None = None
    surprisal_election: bool = False
    refeed_permission: bool = False
    column_permission: bool = False
    identity: list[Message]
    tunable_values: dict[str, Finite]
class ClassifyInstruction(Config):
    model_binding: ModelBinding
class SpuInstruction(Config):
    decoder: DecoderInstruction
    classify: ClassifyInstruction | None = None
class SegmentPreamble(Wire):
    model_config = ConfigDict(strict=True, extra='forbid')
    segments: U64
    bytes: U64

def variant(name, **fields):
    return create_model(''.join(x.title() for x in name.split('_')), __base__=Wire,
                        kind=(Literal[name], ...), **fields)
def tagged(*models):
    return TypeAdapter(Annotated[Union[tuple(models)], Field(discriminator='kind')])

TOKEN_DIRECTIVE = tagged(
    variant('open', session=(str, ...), messages=(list[Message], ...), column_ask=(bool, False)),
    variant('append_and_generate', turn=(str, ...), delta=(list[Message], ...)),
    variant('re_feed', turn=(str, ...), rendered=(str, ...), path=(list[U32], ...)),
    variant('cancel', turn=(str, ...)), variant('flush', keep=(U64, ...)),
    variant('elide', **{'from': (U64, ...), 'to': (U64, ...)}))
TOKEN_REFUSAL = tagged(*[variant(k) for k in (
    'not_open', 'out_of_order', 'malformed_delta', 'refeed_permission_absent',
    'refeed_path_empty', 'column_permission_absent', 'column_readout_unelected', 'column_undeclared')],
    variant('overflow', resident=(U64, ...), requested=(U64, ...), capacity=(U64, ...)),
    variant('unremovable_span', **{k:(U64, ...) for k in ('from','to','prefix','resident')}))
class Generation(Wire):
    emission: str
    finish: Literal['completed','stopped','length']
    content: list[Block]
    request: object
    measurement: object
    resident: U64
    capacity: U64
class TokenBody(Wire):
    token: U32
    piece: str
class Candidate(Wire):
    token: U32
    probability: Finite
class FieldBody(Wire):
    position: U64
    ranked: list[Candidate]
    realized: U32
class ColumnBody(Wire):
    position: U64
    layers: list[list[Finite]]
class CountBody(Wire):
    resident_before: U64
    resident_after: U64
TOKEN_ANSWER = tagged(*[variant(k) for k in ('opened','at_rest')],
    *[variant(k, body=(t, ...)) for k,t in [('token',TokenBody), ('field',FieldBody),
     ('column',ColumnBody), ('generated',Generation), ('re_fed',Generation),
     ('flushed',CountBody), ('elided',CountBody)]])
LABEL_DIRECTIVE = tagged(variant('classify', turn=(str | None,None), content=(str,...)))
class ScoredLabel(Wire):
    label: str
    score: Finite
class ScoreBody(Wire):
    turn: str | None = None
    labels: list[ScoredLabel]
LABEL_ANSWER = tagged(variant('ready'),variant('scored',body=(ScoreBody,...)))
LABEL_REFUSAL = tagged(variant('not_admitted',reason=(str,...)),variant('not_ready'),
                      variant('oversized',requested=(U64,...),bound=(U64,...)),variant('malformed_content'))
class Exchange(Wire):
    opener: Literal['admin','harness','spu','gate']
    ordinal: U64
class Envelope(Wire):
    exchange: Exchange
    position: Literal['open','continue','close']
    payload: dict

ADAPTERS = {'TokenDirective':TOKEN_DIRECTIVE,'TokenAnswer':TOKEN_ANSWER,
            'TokenRefusal':TOKEN_REFUSAL,'LabelDirective':LABEL_DIRECTIVE,
            'LabelAnswer':LABEL_ANSWER,'LabelRefusal':LABEL_REFUSAL,
            'SpuInstruction':TypeAdapter(SpuInstruction), 'Generation':TypeAdapter(Generation),
            'SegmentPreamble':TypeAdapter(SegmentPreamble)}

def dump(value):
    # Only optional config members have Rust's skip_serializing_if rule.
    result = value.model_dump(by_alias=True)
    def clean(obj):
        if isinstance(obj,dict):
            return {k:clean(v) for k,v in obj.items()
                    if not (v is None and k in ('field-election','classify'))}
        if isinstance(obj,list): return [clean(v) for v in obj]
        return obj
    return clean(result)
