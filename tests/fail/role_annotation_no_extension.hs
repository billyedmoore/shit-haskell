{-# LANGUAGE NoRoleAnnotations, GHC2024 #-}

type role Box nominal
newtype Box a = Box a

main = pure ()
