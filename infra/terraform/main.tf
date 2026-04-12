terraform {
  required_version = ">= 1.5.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.38"
    }
  }
}

provider "aws" {
  region  = "us-west-2"
  profile = "mongo"

  ignore_tags {
    keys = [
      "mongodb:infosec:creationTime",
      "mongodb:infosec:lastModifiedTime",
      "mongodb:infosec:creatorIAMRole",
      "mongodb:infosec:creatorIAMUser",
      "mongodb:infosec:creator",
      "mongodb:infosec:WhatIsThis",
      "OwnerContact"
    ]
  }
}

# -------------------------------------------------------------------
# Ubuntu 24.04 LTS (Noble) AMI
# -------------------------------------------------------------------
data "aws_ssm_parameter" "ubuntu_2404_ami" {
  name = "/aws/service/canonical/ubuntu/server/24.04/stable/current/amd64/hvm/ebs-gp3/ami-id"
}

# -------------------------------------------------------------------
# VPC lookup
# -------------------------------------------------------------------
data "aws_vpc" "vetta_vpc" {
  id = var.vetta_vpc_id
}

# -------------------------------------------------------------------
# Subnet lookup
# -------------------------------------------------------------------
data "aws_subnet" "vetta_public" {
  filter {
    name   = "tag:Name"
    values = ["vetta-subnet-public1-us-west-2a"]
  }

  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.vetta_vpc.id]
  }
}

# -------------------------------------------------------------------
# EC2 Instance & Security Group
# -------------------------------------------------------------------
resource "aws_security_group" "vetta_server_security_group" {
  name   = "vetta_security_group"
  vpc_id = data.aws_vpc.vetta_vpc.id

  tags = {
    Name = "vetta_security_group"
  }

  revoke_rules_on_delete = true


}

resource "aws_security_group_rule" "allow_ssh" {
  type              = "ingress"
  from_port         = 22
  to_port           = 22
  protocol          = "tcp"
  cidr_blocks       = var.allowed_ssh_ips
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow SSH from specific IPs"
}

resource "aws_security_group_rule" "https" {
  type              = "egress"
  from_port         = 443
  to_port           = 443
  protocol          = "tcp"
  cidr_blocks       = var.allowed_web_egress_cidrs
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow HTTPS outbound traffic"
}

resource "aws_security_group_rule" "http" {
  type              = "egress"
  from_port         = 80
  to_port           = 80
  protocol          = "tcp"
  cidr_blocks       = var.allowed_web_egress_cidrs
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow HTTP outbound traffic"
}

resource "aws_security_group_rule" "dns_udp" {
  type              = "egress"
  from_port         = 53
  to_port           = 53
  protocol          = "udp"
  cidr_blocks       = [format("%s/32", cidrhost(data.aws_vpc.vetta_vpc.cidr_block, 2))]
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow DNS (UDP) to VPC resolver"
}

resource "aws_security_group_rule" "dns_tcp" {
  type              = "egress"
  from_port         = 53
  to_port           = 53
  protocol          = "tcp"
  cidr_blocks       = [format("%s/32", cidrhost(data.aws_vpc.vetta_vpc.cidr_block, 2))]
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow DNS (TCP) to VPC resolver"
}

resource "aws_security_group_rule" "mongodb_egress" {
  type              = "egress"
  from_port         = 27017
  to_port           = 27017
  protocol          = "tcp"
  cidr_blocks       = ["0.0.0.0/0"]
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow outbound traffic to MongoDB Atlas"
}

resource "aws_security_group_rule" "allow_icmp_pmtud" {
  type              = "ingress"
  from_port         = 3 # ICMP Type 3: Destination Unreachable
  to_port           = 4 # ICMP Code 4: Fragmentation Needed
  protocol          = "icmp"
  cidr_blocks       = ["0.0.0.0/0"]
  security_group_id = aws_security_group.vetta_server_security_group.id
  description       = "Allow inbound ICMP Fragmentation Needed for Path MTU Discovery"
}

resource "aws_instance" "vetta_ec2" {
  ami                         = data.aws_ssm_parameter.ubuntu_2404_ami.value
  associate_public_ip_address = true
  instance_type               = var.instance_type
  key_name                    = var.ec2_kp_name
  subnet_id                   = data.aws_subnet.vetta_public.id
  user_data_base64            = filebase64("${path.module}/../ec2/init.sh")
  user_data_replace_on_change = true
  vpc_security_group_ids      = [aws_security_group.vetta_server_security_group.id]
  disable_api_termination     = true

  root_block_device {
    volume_size           = 60
    volume_type           = "gp3"
    delete_on_termination = true
    encrypted             = true
  }

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 1
  }

  tags = {
    Name = "vetta-server-1"
  }
}
