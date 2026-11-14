{-# LANGUAGE PatternSynonyms, ViewPatterns #-}
-- A bidirectional pattern synonym whose builder uses one defined after it, itself with an
-- explicit builder (GHC's `pattern BinOpApp x op y = OpVal op `App` x `App` y`).
module Main where

data Expr = Var String | App Expr Expr | Lit Int deriving Show

primOp :: String -> Maybe Char
primOp "+" = Just '+'
primOp "*" = Just '*'
primOp _ = Nothing

pattern BinOpApp :: Expr -> Char -> Expr -> Expr
pattern BinOpApp x op y = OpVal op `App` x `App` y

pattern OpVal :: Char -> Expr
pattern OpVal op <- Var (primOp -> Just op) where
  OpVal op = Var [op]

eval :: Expr -> Int
eval (Lit n) = n
eval (BinOpApp x '+' y) = eval x + eval y
eval (BinOpApp x '*' y) = eval x * eval y
eval e = error (show e)

main :: IO ()
main = do
  let e = BinOpApp (Lit 2) '*' (BinOpApp (Lit 3) '+' (Lit 4))
  print e
  print (eval e)
