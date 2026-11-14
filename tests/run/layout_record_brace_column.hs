{-# LANGUAGE GADTs #-}
-- A record's closing brace at the layout column of the enclosing block (GHC's CmmNode): an
-- explicit brace opens a context without layout, so that line does not start a new item.
module Main where

data Node where
  Branch :: Int -> Node
  CondBranch :: {
      pred_ :: Bool,
      onTrue, onFalse :: Int
  } -> Node

data Plain = Plain {
  fieldA :: Int,
  fieldB :: String
}

describe :: Node -> String
describe (Branch n) = "branch " ++ show n
describe c = "cond " ++ show (pred_ c, onTrue c, onFalse c)

main :: IO ()
main = do
  putStrLn (describe (Branch 1))
  putStrLn (describe (CondBranch { pred_ = True, onTrue = 2, onFalse = 3 }))
  let p = Plain {
    fieldA = 4,
    fieldB = "b"
  }
  print (fieldA p, fieldB p)
