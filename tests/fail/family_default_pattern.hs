{-# LANGUAGE TypeFamilies #-}

class C a where
  type Result a b
  type Result a Int = Bool

main = pure ()
