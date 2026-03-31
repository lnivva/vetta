# Cloud Deployment (EC2 / Terraform)

This page covers running Vetta on a remote Linux instance. GPU instances (e.g., `g6.xlarge`) are recommended for faster
Whisper inference.

## Manual EC2 Setup

1. Launch an Ubuntu 24.04+ instance (at least 8 GB RAM, 30 GB disk).
2. SSH into the instance and clone the repository:
    ```bash
    git clone https://github.com/lnivva/vetta
    cd vetta
    ```
3. Run the init script to install all system dependencies (Rust, uv, protoc, ffmpeg, NVIDIA drivers):
   ```bash
   chmod +x infra/ec2/init.sh
   sudo ./infra/ec2/init.sh
   ```
4. Reboot the instance for NVIDIA drivers to take effect:
   ```bash
   sudo reboot
   ```
5. After reconnecting, verify the setup:
   ```bash
   rustc --version
   uv --version
   protoc --version
   ffmpeg -version
   nvidia-smi        # Should show your GPU if on a GPU instance
   ```

## Terraform

A reference Terraform module is provided in `infra/terraform/`. The module provisions an EC2 instance with its own
security group and attaches the init script as user data.

The init script runs automatically on first boot and installs all system dependencies (NVIDIA drivers, Rust, uv, protoc,
ffmpeg), so there are no manual installation steps — just wait for it to finish and verify.

### Prerequisites

| Resource     | Why                      | Notes                                                                            |  
|--------------|--------------------------|----------------------------------------------------------------------------------|  
| VPC          | Network for the instance | Pass its ID via the `vetta_vpc_id` variable                                      |  
| Subnet       | Public subnet            | Must be tagged `Name = vetta-subnet-public1-us-west-2a` with an internet gateway |  
| EC2 Key Pair | SSH access               | Create in the AWS console or with `aws ec2 create-key-pair`                      |  
| AWS Profile  | Authentication           | A profile named `mongo` configured in your AWS credentials                       |  

### Variables

| Variable          | Description                                | Example                 |  
|-------------------|--------------------------------------------|-------------------------|  
| `instance_type`   | EC2 instance type (NVIDIA GPU recommended) | `g6.xlarge`             |  
| `ec2_kp_name`     | Name of an existing EC2 key pair           | `my-key-pair`           |  
| `vetta_vpc_id`    | ID of the VPC to deploy into               | `vpc-0abc1234def56789a` |  
| `allowed_ssh_ips` | List of CIDRs allowed to SSH (port 22)     | `["203.0.113.10/32"]`   |  

### Deploy

```bash
cd infra/terraform  
terraform init  
terraform apply \  
  -var='instance_type=g6.xlarge' \  
  -var='ec2_kp_name=my-key-pair' \  
  -var='vetta_vpc_id=vpc-0abc1234def56789a' \  
  -var='allowed_ssh_ips=["203.0.113.10/32"]'  
```

Or create a `terraform.tfvars` file to avoid passing flags every time:

```hcl
# infra/terraform/terraform.tfvars  

instance_type = "g6.xlarge"
ec2_kp_name   = "my-key-pair"
vetta_vpc_id  = "vpc-0abc1234def56789a"
allowed_ssh_ips = ["203.0.113.10/32"]  
```

```bash
cd infra/terraform  
terraform init  
terraform apply  
```

### Post-Deploy

After `terraform apply` completes, the public IP is printed as an output:

```text
Apply complete! Resources: <n> added, 0 changed, 0 destroyed.  

Outputs:  

public_ip = "44.230.XXX.XXX"  
```

You can retrieve it again at any time with:

```bash
terraform output public_ip  
```

SSH into the instance:

```bash
ssh -i ~/.ssh/<your-key-pair-name>.pem ubuntu@$(terraform output -raw public_ip)  
```

The init script may take several minutes to complete on first boot (driver installation, package downloads). Wait for
cloud-init to finish before using the instance:

```bash
cloud-init status --wait  
```

Once it reports `status: done`, verify that everything was installed correctly:

```bash  
rustc --version  
uv --version  
protoc --version  
ffmpeg -version  
nvidia-smi        # Should show your GPU if on a GPU instance  
```

::: tip  
If you need to debug a failed init, check the full cloud-init log:

```bash
tail -f /var/log/cloud-init-output.log  
```

:::

::: warning  
`/etc/environment` is readable by all users on the instance. For production deployments, prefer injecting secrets
through **AWS Secrets Manager**, **SSM Parameter Store**, or your CI/CD pipeline rather than writing credentials to
disk.  
:::
