output "public_ip" {
  description = "Public IP address of the Vetta EC2 instance"
  value       = aws_instance.vetta_ec2.public_ip
}  
