terraform {
  required_version = ">= 1.5.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 4.0"
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
# Subnet lookup
# -------------------------------------------------------------------
data "aws_subnet" "vetta_public" {
  filter {
    name   = "tag:Name"
    values = ["vetta-subnet-public1-us-west-2a"]
  }
}

# -------------------------------------------------------------------
# EC2 Instance
# -------------------------------------------------------------------
locals {
  init_script = "${path.module}/../ec2/init.sh"
}

resource "aws_instance" "vetta_ec2" {
  ami                         = data.aws_ssm_parameter.ubuntu_2404_ami.value
  associate_public_ip_address = true
  instance_type               = "g6.xlarge"
  key_name                    = "lfreitas-mongo-kp"
  subnet_id                   = data.aws_subnet.vetta_public.id
  user_data                   = file(local.init_script)
  user_data_replace_on_change = true
  vpc_security_group_ids = [
    "sg-0b9cb19902c1307fb"
  ]
  root_block_device {
    volume_size           = 60
    volume_type           = "gp3"
    delete_on_termination = true
  }

  tags = {
    Name = "vetta-server-1"
  }
}
