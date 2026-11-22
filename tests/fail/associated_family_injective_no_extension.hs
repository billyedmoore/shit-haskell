{-# LANGUAGE TypeFamilies, NoTypeFamilyDependencies #-}

class C a where
  type Result a = r | r -> a

main = pure ()
