module Data.Kind (Type, Constraint) where

import Base

-- Kinds are not checked against these names; they exist so that signatures can mention them.
data Type
data Constraint
