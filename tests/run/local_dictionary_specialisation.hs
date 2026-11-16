-- Local polymorphic functions specialised at their single type (dictionary arguments dropped).
main :: IO ()
main = do
  let sq x = x * x
      go acc n = if n == 0 then acc else go (acc + sq n) (n - 1)
      pairs = [(a, b) | a <- [1 .. 3], b <- "xy"]
      count p = length . filter p
      twice f = f . f
      member e = any (== e)
  print (go 0 (100 :: Int), sq (1.5 :: Double))
  print (pairs, count even [1 .. 10 :: Int], twice (+ 1) (5 :: Integer))
  print (member 'q' "queue", member (3 :: Int) [1, 2], map (twice sq) [2 :: Int, 3])
  print (let showAll xs = map show xs in (showAll [1, 2 :: Int], showAll [True]))
  -- Recursive, with constant dictionaries before several other arguments (at two element types).
  let rep x k = if k <= 0 then [] else x : rep x (k - 1)
      walk f z k = if k == 0 then z else walk f (f z k) (k - 1)
  print (rep 'a' (3 :: Int), length (rep True (4 :: Int)), walk (+) 0 (10 :: Int), walk (\s _ -> s ++ "!") "go" (3 :: Int))
