{-# LANGUAGE TypeFamilies #-}

class C a where
  type Result a

type instance Result Int = Bool

main = pure ()
