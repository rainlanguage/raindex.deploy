output "droplet_id" {
  description = "ID of the Raindex API droplet"
  value       = digitalocean_droplet.nixos.id
}

output "droplet_ipv4" {
  description = "Ephemeral IPv4 address of the droplet"
  value       = digitalocean_droplet.nixos.ipv4_address
}

output "reserved_ip" {
  description = "Stable public IP to use for DNS and deployment"
  value       = digitalocean_reserved_ip.nixos.ip_address
}

output "volume_id" {
  description = "Persistent Raindex API data volume"
  value       = digitalocean_volume.data.id
}
