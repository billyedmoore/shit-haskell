{-# LANGUAGE TypeFamilies #-}

data family Storage a

type family Stored a where
  Stored (Storage a) = Bool

answer :: Stored (Storage Int)
answer = True

main :: IO ()
main = print answer
