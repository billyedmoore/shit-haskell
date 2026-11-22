{-# LANGUAGE TypeFamilies #-}

class C a where
  type Result a b
  type Result a _ = Bool

instance C Int

answer :: Result Int Char
answer = True

main :: IO ()
main = print answer
