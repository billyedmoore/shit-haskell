{-# LANGUAGE TemplateHaskell #-}
-- Expression splices run at compile time: the syntax they build takes their place.
module Main where

import Language.Haskell.TH
main :: IO ()
main = do
  print ($(litE (integerL 42)) :: Int, $(stringE "hi"))
  print ($(appE (varE 'succ) (litE (integerL 9))) :: Integer)
  print ($(lamE [varP (mkName "x")] (infixE (Just (varE (mkName "x"))) (varE '(+)) (Just (litE (integerL 1))))) (2 :: Int))
