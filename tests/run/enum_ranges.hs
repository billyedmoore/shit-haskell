main :: IO ()
main = do
  print [1, 3 .. 11 :: Int]
  print ['a' .. 'e']
  print [10, 8 .. 1 :: Int]
  print [5 .. 1 :: Int]
  print [0.1, 0.2 .. 0.5 :: Double]
  print [1.0 .. 3.5 :: Double]
  print (take 3 ['x' ..])
  print [LT ..]
  print (succ 'a', pred 10 :: Int, [False ..])
