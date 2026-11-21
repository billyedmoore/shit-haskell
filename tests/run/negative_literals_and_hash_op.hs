{-# LANGUAGE NegativeLiterals #-}

(##) :: Int -> Int -> Int
x ## y = x * 10 + y

apply :: (Int -> Int) -> Int -> Int
apply f x = f x

main :: IO ()
main = do
  print (apply (+ 5) -3)
  print ((##) 4 -2)
  print (negate -7)
