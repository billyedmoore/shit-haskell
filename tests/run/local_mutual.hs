isEven', isOdd' :: Int -> Bool
isEven' 0 = True
isEven' n = isOdd' (n - 1)
isOdd' 0 = False
isOdd' n = isEven' (n - 1)

main :: IO ()
main = do
  print (isEven' 10, isOdd' 7)
  let pairUp x = (x, x)
  print (pairUp 'a', pairUp True)
  let go 0 acc = acc
      go n acc = go (n - 1) (acc + n)
  print (go (100000 :: Int) 0)
  print (let f = (+ 1) . (* 2) in f (5 :: Int))
