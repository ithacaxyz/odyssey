FROM ubuntu

COPY target/release/odyssey /usr/local/bin

# Copy licenses
COPY LICENSE-* ./

# Copy the genesis files
ADD etc/dev-genesis.json ./etc/dev-genesis.json
ADD etc/odyssey-genesis.json ./etc/odyssey-genesis.json

EXPOSE 30303 30303/udp 9001 8545 9000 8546
ENTRYPOINT ["/usr/local/bin/odyssey"]
