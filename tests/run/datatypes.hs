data Color = Red | Green | Blue deriving (Show, Eq, Ord, Enum, Bounded)

data Shape = Circle Double | Rect { width :: Double, height :: Double }
  deriving (Show, Eq)

data Tree a = Leaf | Node (Tree a) a (Tree a) deriving (Show)

data Pair a b = Pair a b deriving (Show, Eq, Ord)

insert :: Ord a => a -> Tree a -> Tree a
insert x Leaf = Node Leaf x Leaf
insert x t@(Node l v r)
  | x < v = Node (insert x l) v r
  | x > v = Node l v (insert x r)
  | otherwise = t

toList :: Tree a -> [a]
toList Leaf = []
toList (Node l v r) = toList l ++ [v] ++ toList r

area :: Shape -> Double
area (Circle r) = pi * r * r
area (Rect w h) = w * h

main :: IO ()
main = do
  print [minBound .. maxBound :: Color]
  print (succ Red, fromEnum Blue, toEnum 1 :: Color)
  print (Circle 1.5, Rect { width = 2, height = -3 })
  print (area (Rect 2 3))
  print (toList (foldr insert Leaf [5, 2, 8, 1, 9, 3 :: Int]))
  print (foldr insert Leaf [2, 1 :: Int])
  print (Pair (-1 :: Int) (Just (-2.5 :: Double)))
  print (compare (Pair 1 'a') (Pair 1 'b'), Red < Blue, maximum [Green, Red])
