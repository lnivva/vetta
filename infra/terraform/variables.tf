variable "instance_type" {
  description = "Vetta Server EC2 Instance Type (Nvidia GPU enabled)"
  nullable    = false
  type        = string
}

variable "ec2_kp_name" {
  description = "EC2 key pair name"
  nullable    = false
  type        = string
}

variable "vetta_vpc_id" {
  description = "Vetta VPC ID"
  nullable    = false
  type        = string
}

variable "allowed_ssh_ips" {
  description = "List of IPs allowed to SSH"
  type        = list(string)
  nullable    = false
}
