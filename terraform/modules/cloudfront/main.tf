variable "environment" { type = string }
variable "domain_name" { type = string }
variable "certificate_arn" { type = string }
variable "bucket_regional_name" { type = string }
variable "tags" { type = map(string), default = {} }
