variable "do_token" {
  description = "DigitalOcean API token"
  type        = string
  sensitive   = true
}

variable "ssh_key_name" {
  description = "DigitalOcean SSH key installed on the API host"
  type        = string
  default     = "raindex-op"
}

variable "region" {
  description = "DigitalOcean region"
  type        = string
  default     = "nyc3"
}

variable "droplet_size" {
  description = "DigitalOcean droplet size slug"
  type        = string
  default     = "s-2vcpu-4gb"
}

variable "volume_size_gb" {
  description = "Persistent indexer and log volume size"
  type        = number
  default     = 10
}
