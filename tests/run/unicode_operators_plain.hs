-- Without UnicodeSyntax, Unicode symbols are ordinary operators: a program may define ★.
module Main where
infixl 6 ★
(★) :: Int -> Int -> Int
a ★ b = a * 10 + b
main :: IO ()
main = print (1 ★ 2 ★ 3, 2 * 3)
