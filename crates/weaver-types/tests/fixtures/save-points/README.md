# The save-point corpus

One corpus, two readers: `weaver-state`'s `SavePoint::parse` and `weaver-admin`'s
`save_points::judge` are hand-written readers of one format, `weaver-state-Spec`
section 3, and admin links no interior crate, so each crate's test reads every file
here and asserts the verdict `verdicts.txt` gives it (`sound` or `refuses`). A reader
that admits what the other refuses fails its own test, which is the drift this corpus
pins. `sound.save-point` is the format exactly as the member writes it; each other file
is malformed one way, named by its file name.
