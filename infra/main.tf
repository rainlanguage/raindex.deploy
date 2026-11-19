resource "digitalocean_ssh_key" "operator" {
  name       = var.ssh_key_name
  public_key = trimspace(file("${path.module}/raindex-op.pub"))
}

resource "digitalocean_volume" "data" {
  region                  = var.region
  name                    = "raindex-api-data"
  size                    = var.volume_size_gb
  initial_filesystem_type = "ext4"
  description             = "Persistent Raindex indexer database and rotated logs"
}

resource "digitalocean_droplet" "nixos" {
  image    = "ubuntu-24-04-x64"
  name     = "raindex-api-nixos"
  region   = var.region
  size     = var.droplet_size
  ssh_keys = [digitalocean_ssh_key.operator.id]
}

resource "digitalocean_volume_attachment" "data" {
  droplet_id = digitalocean_droplet.nixos.id
  volume_id  = digitalocean_volume.data.id
}

resource "digitalocean_reserved_ip" "nixos" {
  region = var.region
}

resource "digitalocean_reserved_ip_assignment" "nixos" {
  ip_address = digitalocean_reserved_ip.nixos.ip_address
  droplet_id = digitalocean_droplet.nixos.id

  depends_on = [digitalocean_volume_attachment.data]
}
