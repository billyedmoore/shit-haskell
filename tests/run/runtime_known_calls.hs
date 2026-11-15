module Main where
import Control.Exception

{-# NOINLINE add3 #-}
add3 :: Int -> Int -> Int -> Int
add3 a b c = a + b + c

{-# NOINLINE evenCall #-}
evenCall :: Int -> Bool
evenCall n = if n == 0 then True else oddCall (n - 1)
{-# NOINLINE oddCall #-}
oddCall :: Int -> Bool
oddCall n = if n == 0 then False else evenCall (n - 1)

{-# NOINLINE rotate #-}
rotate :: Int -> Int -> Int -> (Int, Int)
rotate a b n = if n == 0 then (a, b) else rotate b (a + b) (n - 1)

{-# NOINLINE choose #-}
choose :: Bool -> Int -> Int
choose b = if b then (+ 1) else (* 2)
{-# NOINLINE funCAF #-}
funCAF :: Int -> Int
funCAF = let one = 1 in \x -> x + one
{-# NOINLINE first #-}
first :: Int -> Int -> Int
first a _ = a

main :: IO ()
main = do
  print (add3 1 2 3, add3 (add3 1 2 3) 4 5)
  print (evenCall 10000, oddCall 10001, rotate 1 2 20)
  let partial = add3 10
      again = partial 20
  print (again 30, map (add3 1 2) [3 .. 6])
  print (choose True 5, choose False 5, funCAF 7)
  let local n = add3 40 n 2
  print (map local [1 .. 4])
  print (first 42 (error "unused"))
  result <- try (evaluate (add3 1 (error "forced") 3)) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) result)
  print (add3 7 8 9)
