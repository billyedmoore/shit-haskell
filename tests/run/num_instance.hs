data V2 = V2 Int Int deriving (Show, Eq)

instance Num V2 where
  V2 a b + V2 c d = V2 (a + c) (b + d)
  V2 a b * V2 c d = V2 (a * c) (b * d)
  abs (V2 a b) = V2 (abs a) (abs b)
  signum (V2 a b) = V2 (signum a) (signum b)
  fromInteger n = V2 (fromInteger n) (fromInteger n)
  negate (V2 a b) = V2 (negate a) (negate b)

main :: IO ()
main = do
  print (V2 1 2 + 10)
  print (sum [V2 1 1, V2 2 3])
  print (negate (V2 4 (-4)) - 1)
