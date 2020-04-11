import sys
import os
import json
import subprocess

keypair_per_node = 1
if len(sys.argv) >= 2:
    keypair_per_node = int(sys.argv[1])

prism_bin = "../target/release/prism"
os.makedirs("keypairs", exist_ok=True)

fund_addrs = []
keypairs = []
for idx in range(keypair_per_node):
    result = subprocess.run([prism_bin, "keygen", "--addr"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    #result = subprocess.run([prism_bin, "keygen", "--addr"], capture_output=True, text=True)#python 3.7
    keypair = result.stdout
    address = result.stderr
    keypairs.append(keypair)
    fund_addrs.append("--fund-addr {}".format(address.strip()))
    with open("keypairs/{}.pkcs8".format(idx), "w") as f:
        f.write(keypair.strip())
fund_opt = " ".join(fund_addrs)
with open("keypairs/fund_addr.txt", "w") as f:
    f.write(fund_opt)
