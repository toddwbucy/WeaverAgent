"""The prompt's block partition, weaver-spu measurement.rs `PromptPartition`, ported:
the offsets are each token's end in the rendered text, and they partition it exactly or
are refused."""

class PartitionDefect(ValueError):
    """measurement.rs `PartitionDefect`: `short_of_end` with the last offset and the text
    length, `not_ascending` with the index, or `empty`."""
    def __init__(self,kind,**fields):
        self.kind=kind; self.fields=fields; super().__init__(kind)

def prompt_partition(offsets,text_len):
    """`PromptPartition::new`: no offsets partition only the empty text; the offsets
    ascend, equal neighbours allowed; the last equals the text's length. Answers the
    offsets, the text length and the block count."""
    if not offsets:
        if text_len==0: return {'offsets':[],'text_len':0,'blocks':0}
        raise PartitionDefect('empty')
    for at in range(1,len(offsets)):
        if offsets[at]<offsets[at-1]: raise PartitionDefect('not_ascending',at=at)
    if offsets[-1]!=text_len: raise PartitionDefect('short_of_end',last=offsets[-1],text_len=text_len)
    return {'offsets':list(offsets),'text_len':text_len,'blocks':len(offsets)}
