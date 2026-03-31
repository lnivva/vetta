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

  validation {
    condition = length(var.allowed_ssh_ips) > 0 && alltrue([
      for cidr in var.allowed_ssh_ips : can(cidrhost(cidr, 0))
    ])
    error_message = "allowed_ssh_ips must be a non-empty list of valid CIDR blocks."
  }
}

variable "allowed_web_egress_cidrs" {
  description = "List of IPs allowed to egress"
  type        = list(string)
  nullable    = false

  validation {
    condition = length(var.allowed_ssh_ips) > 0 && alltrue([
      for cidr in var.allowed_web_egress_cidrs : can(cidrhost(cidr, 0))
    ])
    error_message = "allowed_web_egress_cidrs must be a non-empty list of valid CIDR blocks."
  }
}
