terraform {
  required_version = ">= 1.6"
  backend "s3" {
    bucket = "tf-state"
    key    = "app/terraform.tfstate"
    region = "eu-west-1"
  }
}

provider "aws" {
  region = var.region
}

provider "aws" {
  alias  = "us"
  region = "us-east-1"
}

variable "region" {
  type    = string
  default = "eu-west-1"

  validation {
    condition     = can(regex("^eu-", var.region))
    error_message = "Only EU regions."
  }
}

variable "subnets" {
  type = map(object({
    cidr = string
    az   = string
  }))
  default = {
    a = { cidr = "10.0.1.0/24", az = "eu-west-1a" }
    b = { cidr = "10.0.2.0/24", az = "eu-west-1b" }
  }
}

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"]

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
}

locals {
  env         = terraform.workspace == "default" ? "dev" : terraform.workspace
  name        = "app-${local.env}"
  azs         = [for s in var.subnets : s.az]
  cidr_by_az  = { for k, s in var.subnets : s.az => s.cidr }
  port        = 8080
  ratio       = 0.75
  enabled     = true
  tags = {
    Project = "app"
    Env     = local.env
  }
}

module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "5.1.0"

  name = local.name
  cidr = "10.0.0.0/16"
  azs  = local.azs

  enable_nat_gateway = true
  tags               = local.tags
}

resource "aws_security_group" "web" {
  name   = "${local.name}-web"
  vpc_id = module.vpc.vpc_id

  dynamic "ingress" {
    for_each = [80, 443]
    content {
      from_port   = ingress.value
      to_port     = ingress.value
      protocol    = "tcp"
      cidr_blocks = ["0.0.0.0/0"]
    }
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_instance" "web" {
  for_each      = var.subnets
  ami           = data.aws_ami.ubuntu.id
  instance_type = "t3.small"
  subnet_id     = module.vpc.private_subnets[0]

  user_data = <<EOT
#!/bin/bash
echo "port ${local.port}" > /etc/app.conf
EOT

  tags = merge(local.tags, { Name = "${local.name}-${each.key}" })

  lifecycle {
    create_before_destroy = true
    ignore_changes        = [tags["LastSeen"]]
  }
}

resource "aws_instance" "worker" {
  count         = 2
  ami           = data.aws_ami.ubuntu.id
  instance_type = "t3.medium"
  # plain comment
  monitoring = false
}

output "web_ips" {
  value       = { for k, i in aws_instance.web : k => i.private_ip }
  description = "Private IPs"
}

output "policy" {
  value = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["s3:GetObject"]
      Resource = "*"
    }]
  })
}
