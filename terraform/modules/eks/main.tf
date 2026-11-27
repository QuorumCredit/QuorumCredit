variable "environment" { type = string }
variable "cluster_name" { type = string }
variable "cluster_version" { type = string }
variable "vpc_id" { type = string }
variable "subnet_ids" { type = list(string) }
variable "node_groups" { type = any }
variable "cluster_enabled_log_types" { type = list(string), default = [] }
variable "cluster_log_retention_in_days" { type = number, default = 30 }
variable "tags" { type = map(string), default = {} }

output "cluster_name" { value = var.cluster_name }
output "cluster_endpoint" { value = "https://eks.placeholder.endpoint" }
output "worker_security_group_id" { value = "sg-worker-placeholder" }
