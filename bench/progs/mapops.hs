import Data.List (foldl')

-- Inserts, lookups and a fold over an unbalanced search tree with pseudo-random keys.
data Tree = Leaf | Node Tree !Int !Int Tree

insert :: Int -> Int -> Tree -> Tree
insert k v Leaf = Node Leaf k v Leaf
insert k v t@(Node l k' v' r)
  | k < k' = Node (insert k v l) k' v' r
  | k > k' = Node l k' v' (insert k v r)
  | otherwise = Node l k v r

member :: Int -> Tree -> Bool
member _ Leaf = False
member k (Node l k' _ r)
  | k < k' = member k l
  | k > k' = member k r
  | otherwise = True

size :: Tree -> Int
size Leaf = 0
size (Node l _ _ r) = size l + 1 + size r

total :: Tree -> Int
total Leaf = 0
total (Node l _ v r) = total l + v + total r

main :: IO ()
main = do
  let n = 50000 :: Int
      t = foldl' (\acc k -> insert ((k * 7919) `mod` 65521) k acc) Leaf [1 .. n]
      hits = length [() | k <- [1 .. n], member k t]
  print (size t, hits, total t)
