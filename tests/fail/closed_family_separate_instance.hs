{-# LANGUAGE TypeFamilies #-}

type family F a where
  F Int = Bool

type instance F Char = Int

main :: IO ()
main = pure ()
