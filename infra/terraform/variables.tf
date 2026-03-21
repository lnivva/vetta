variable "security_group" {
  description = "Vetta Server Security Group"
  nullable    = false
  type        = set(string)
}

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
