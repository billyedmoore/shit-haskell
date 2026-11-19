-- A pattern synonym whose right side has a negative literal (network's SocketType (-1)).
{-# LANGUAGE PatternSynonyms #-}
module Main (main) where
newtype T = T Int deriving Show
pattern Neg :: T
pattern Neg = T (-1)
f :: T -> String
f Neg = "neg"
f (T n) = show n
main = mapM_ (putStrLn . f) [Neg, T 3, T (-1)]
