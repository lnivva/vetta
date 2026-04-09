# import os
# import voyageai
# from pymongo import MongoClient, UpdateOne
# import time
#
# VOYAGE_API_KEY = os.environ["VOYAGE_API_KEY"]
# MONGODB_URI = os.environ["MONGODB_URI"]
# MODEL = "voyage-finance-2"
# BATCH_SIZE = 20
#
#
# def main():
#     vo = voyageai.Client(api_key=VOYAGE_API_KEY)
#     mongo = MongoClient(MONGODB_URI)
#     collection = mongo["vetta"]["earnings_chunks"]
#
#     # Find chunks without embeddings
#     query = {
#         "$or": [
#             {"embedding": {"$exists": False}},
#             {"embedding": None}
#         ]
#     }
#
#     total = collection.count_documents(query)
#     print(f"Found {total} chunks to embed")
#
#     if total == 0:
#         print("Nothing to do!")
#         return
#
#     processed = 0
#     batch_num = 0
#     total_batches = (total + BATCH_SIZE - 1) // BATCH_SIZE
#
#     while True:
#         batch = list(collection.find(query).limit(BATCH_SIZE))
#
#         if not batch:
#             break
#
#         batch_num += 1
#
#         # Prepare texts with context
#         texts = []
#         for chunk in batch:
#             text = chunk["text"]
#             prev = chunk.get("context", {}).get("previous_text")
#             if prev:
#                 text = f"[Previous: {prev}] {text}"
#             texts.append(text)
#
#             # Call Voyage AI
#         result = vo.embed(
#             texts=texts,
#             model=MODEL,
#             input_type="document"
#         )
#
#         # Write back to MongoDB
#         ops = [
#             UpdateOne(
#                 {"_id": chunk["_id"]},
#                 {"$set": {
#                     "embedding": result.embeddings[i],
#                     "model_version": MODEL
#                 }}
#             )
#             for i, chunk in enumerate(batch)
#         ]
#
#         bulk_result = collection.bulk_write(ops)
#         processed += bulk_result.modified_count
#
#         print(
#             f"✅ Batch {batch_num}/{total_batches}: {bulk_result.modified_count} chunks embedded ({processed}/{total})")
#
#         time.sleep(0.2)
#
#         # Verify
#     with_emb = collection.count_documents({"embedding": {"$exists": True, "$not": {"$eq": None}}})
#     without_emb = collection.count_documents(query)
#
#     print(f"\n── Done ──")
#     print(f"Total chunks:       {with_emb + without_emb}")
#     print(f"With embeddings:    {with_emb}")
#     print(f"Without embeddings: {without_emb}")
#
#
# if __name__ == "__main__":
#     main()
