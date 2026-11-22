{-# LANGUAGE TypeFamilies #-}

class C a where
  type Result a

class D a

instance D Int where
  type Result Int = Bool

main = pure ()
