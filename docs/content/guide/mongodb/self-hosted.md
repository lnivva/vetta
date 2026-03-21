# Self-Hosted / Existing MongoDB

If you already have a MongoDB instance running (self-managed, Docker, replica set, etc.), just export your connection
details:

```bash
export MONGODB_URI="your-connection-string"  
export MONGODB_DATABASE="vetta"  
```

## Requirements

- MongoDB **8.0+** recommended
- The user in your connection string needs `readWrite` on the target database

## Verify connectivity

```bash
mongosh "$MONGODB_URI" --eval "db.runCommand({ ping: 1 })"  
```

  
