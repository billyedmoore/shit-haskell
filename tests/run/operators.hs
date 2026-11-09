infixr 5 +++
(+++) :: [a] -> [a] -> [a]
xs +++ ys = foldr (:) ys xs

infixl 6 <+>
(<+>) :: Int -> Int -> Int
a <+> b = a * 10 + b

newtype Op = Op { runOp :: Int -> Int }

main :: IO ()
main = do
  print ([1, 2] +++ [3] +++ [4 :: Int])
  print (1 <+> 2 <+> 3)
  print (map (subtract 1) [1, 2, 3 :: Int], map (`div` 2) [7, 9 :: Int], map (2 ^) [1, 2, 3 :: Int])
  print ((`elem` "aeiou") 'e', (.) (+ 1) (* 2) (5 :: Int), ($ 3) (+ (1 :: Int)))
  print (runOp (Op (* 3)) 7)
  print (let (a, b) = (3 :: Int, 4 :: Int) in a * b)
