# pBFT

A Byzantine Fault Tolerant KV-Store. The protocol is implemented as described in https://pmg.csail.mit.edu/papers/osdi99.pdf.

To run a node on a cluster [addr_1, addr_2, ... addr_n] with addr_i = (ip_i):(port_i), first

```
cargo build
```
then 
```
cargo run --bin pbft_node n [addr_1] ... [addr_n] i
```
To run the client,
```
cargo run --bin pbft_client n [addr_1] ... [addr_n] [resp_addr]
```
where resp_addr in the address which nodes will send client responses to.
To issue commands to the cluster as the client, issue set and get commands as "set x 42" and "get x". The commands will be broadcasted to the cluster, and upon receiving a quorum of signed votes from the cluster with the same response value, the op has been committed to the kv store and has been safely replicated.
