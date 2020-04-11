# Prism Distributed Testbed

This directory holds the scripts for running experiments and reproducing the results in the paper.

## Setting Up

### Set Up AWS Account

1. Configure an IAM role with the following permissions
    - DescribeInstances
    - DescribeInstanceStatus
    - CreateTags
    - RunInstances
    - TerminateInstances
2. Create an EC2 key pair
3. Create an EC2 security group that allows all traffic to go in/out
4. Create an EC2 Launch Template with the following configurations
    - AMI: Ubuntu 18.04
    - Instance type: `c5d.4xlarge`
    - Key pair: the one just created
    - Network type: VPC
    - Security Groups: the one just created
    - Storage (Volumes): 32 GiB `gp2` volume, delete on termination
    - Instance tags: Key=prism, Value=distributed-testing, tag instance
5. Create a S3 bucket with name `prism-smart-contract` and set it to be publicly accessible

### Local Machine Requirement

- Ubuntu 18.04 VM
- Rust toolchain (nightly)
- Install `clang`, `build-essential`
- Install `jq` and Golang
- Install AWS CLI tool and configure the IAM Key to be the one just created, and Region to be the closest one
- Install rrdtool

### Preparation

1. Modify `run.sh` to use the Launch Tempate ID of the one just created
2. Place the SSH key just created at `~/.ssh/prism.pem`
3. Place this line `Include config.d/prism` at the beginning of `~/.ssh/config`
4. Execute `mkdir -p ~/.ssh/config.d`
5. Build the telematics tool by `cd telematics && go build`
6. Build Prism by `cd .. && cargo build --release`.

## Usage

Run `./run.sh help` to view a list of available commands.

## Experiment Flow

1. `cd` to `testbed/`
2. Run `./run.sh build` to build the Prism binary
3. Run `./run.sh start-instances 100` to start 100 instances
4. Run `./run.sh tune-tcp`, `./run.sh shape-traffic 120 300000`, `./run.sh mount-nvme` to configure the servers
5. Run `./run.sh gen-keypair 10000` to generate 10000 keypairs, which also means 10000 accounts
6. Run `./run.sh gen-payload randreg_100.json 10000` to generate the payload, where 10000 is the number of keypairs/accounts
7. Run `./run.sh sync-payload` to push the test files to remote servers
8. Run `./run.sh run-exp-contract donothing` to run the experiment
9. Run `telematics/telematics log` to monitor the performance
10. To stop the instances, run `./run.sh stop-instances`

## Log Files

`instances.txt` records the EC2 instances that are started in the following
format:

```
<Instance ID>,<Public IP>,<VPC IP>
```

nodes.txt records the Scorex nodes that are started, in the following format:

```
<Node Name>,<EC2 ID>,<Public IP>,<VPC IP>,<API IP>,<P2P IP>
```

