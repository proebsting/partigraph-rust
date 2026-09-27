# Fixtures

`gerrymandria.json` is GerryChain 1.0.0's bundled `gerrychain.examples.gerrymandria`
graph, written out with `Graph.to_json()`. A 64-node 8x8 grid with population,
county, municipality and district attributes.

It is committed so the tests have a file that a real GerryChain actually wrote,
rather than one this crate invented and could be consistently wrong about. The
tests do not run Python.
